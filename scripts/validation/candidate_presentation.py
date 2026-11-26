"""Match the first candidate draw to its physical presentation on Android."""
import argparse
import csv
import hashlib
import json
import statistics
import subprocess
from pathlib import Path


def display_frames(processor, trace, tokens):
    token_list = ",".join(str(int(token)) for token in tokens)
    query = f"""
        SELECT s.surface_frame_token AS token, s.ts AS started, d.ts+d.dur AS presented,
               s.display_frame_token AS display_token, s.present_type AS status
        FROM actual_frame_timeline_slice s JOIN actual_frame_timeline_slice d
          ON d.display_frame_token = s.display_frame_token
        WHERE s.surface_frame_token IN ({token_list})
          AND s.layer_name GLOB '*InputMethod*' AND d.dur >= 0
          AND (d.surface_frame_token IS NULL OR d.surface_frame_token = 0)
        ORDER BY s.surface_frame_token
    """
    output = subprocess.check_output([str(processor), "-Q", query, str(trace)], text=True, encoding="utf-8")
    frames = {}
    for row in csv.DictReader(output.splitlines()):
        token = int(row["token"])
        if token in frames or "drop" in row["status"].lower():
            raise ValueError(f"Ambiguous or dropped frame {row}")
        frames[token] = row
    if set(frames) != set(tokens):
        raise ValueError(f"Missing app/display frame pairs for {set(tokens) - frames.keys()}")
    return frames


def measure(report, trace, processor):
    trials = report["trials"]
    tokens = [trial["vsync_id"] for trial in trials]
    if len(tokens) != 195 or len(set(tokens)) != len(tokens):
        raise ValueError("Expected three rounds of 65 distinct submitted frames")
    offset = report["boot_minus_monotonic_start_ns"]
    if abs(report["boot_minus_monotonic_end_ns"] - offset) >= 100_000:
        raise ValueError("Clock domains diverged")
    frames = display_frames(processor, trace, tokens)
    samples = []
    for trial in trials:
        row = frames[trial["vsync_id"]]
        # This nanosecond GPU fence identifies the same submitted buffer in both
        # the app's HWUI stats and the system compositor's physical layer history.
        matches = [shown for ready, shown in trial["layer_fences"] if ready == trial["GpuCompleted"]]
        if len(matches) != 1:
            raise ValueError(f"Expected a unique GPU fence for {trial['vsync_id']}")
        shown = matches[0]
        timeline_shown = int(row["presented"]) - offset
        if abs(shown - timeline_shown) >= 100_000 or not (
            trial["down_ns"] <= trial["release_ns"] <= trial["draw_ns"] <= shown
            and int(row["started"]) - offset <= trial["draw_ns"]
        ):
            raise ValueError(f"Physical, input or traversal times disagree for {trial['vsync_id']}")
        samples.append(dict(token=trial["vsync_id"], display_token=int(row["display_token"]), presented_ns=shown,
                            timeline_delta_ns=timeline_shown - shown,
                            dwell_ms=(trial["release_ns"] - trial["down_ns"]) / 1e6,
                            down_ms=(shown - trial["down_ns"]) / 1e6,
                            release_ms=(shown - trial["release_ns"]) / 1e6))
    rounds = []
    for start in range(0, len(samples), 65):
        group = samples[start + 5:start + 65]
        summary = {}
        for field in ["down_ms", "release_ms", "dwell_ms"]:
            values = sorted(sample[field] for sample in group)
            summary[field] = dict(p50=statistics.median(values), p95=values[56], p99=values[59])
        rounds.append(summary)
    return dict(trace_sha256=hashlib.sha256(trace.read_bytes()).hexdigest(), rounds=rounds, samples=samples,
                si_200_300_pass=all(r["down_ms"]["p95"] <= 200 and r["down_ms"]["p99"] <= 300 for r in rounds),
                ak_50_pass=all(r["down_ms"]["p95"] <= 50 for r in rounds))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("trace", type=Path)
    parser.add_argument("processor", type=Path)
    args = parser.parse_args()
    print(json.dumps(measure(json.loads(args.report.read_bytes()), args.trace, args.processor.resolve()), indent=2))
