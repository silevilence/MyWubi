"""Match the first candidate draw to its physical presentation on Android."""
import argparse
import csv
import hashlib
import json
import re
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



def standalone_input(raw, trace, processor):
    """Normalize system input slices and the IME's draw marker to the same monotonic clock."""
    def query(sql):
        result = subprocess.check_output([str(processor), "-Q", sql, str(trace)], text=True, encoding="utf-8")
        return list(csv.DictReader(result.splitlines()))

    clocks = [(int(row["ts"]), int(row["offset"])) for row in query("""
        SELECT ts, ts - clock_value AS offset FROM clock_snapshot WHERE clock_name = 'MONOTONIC'
    """)]

    def offset_at(ts):
        return min(clocks, key=lambda clock: abs(clock[0] - ts))[1]

    ime_main = """
        FROM slice s JOIN thread_track tt ON s.track_id = tt.id
        JOIN thread t USING(utid) JOIN process p USING(upid)
        WHERE t.tid = p.pid AND p.upid IN (
            SELECT upid FROM actual_frame_timeline_slice WHERE layer_name GLOB '*InputMethod*')
    """
    draws = [int(row["ts"]) for row in query("SELECT s.ts " + ime_main +
                                            " AND s.name = 'MyWubi candidate strip' ORDER BY s.ts")]
    events = query("""
        SELECT s.ts, s.dur, s.name, (SELECT name FROM descendant_slice(s.id)
            WHERE name GLOB 'deliverInputEvent src=*' LIMIT 1) AS event
    """ + ime_main + """ AND (s.name GLOB 'dispatchInputEvent MotionEvent ACTION_UP*'
                          OR s.name GLOB 'dispatchInputEvent MotionEvent ACTION_DOWN*') ORDER BY s.ts""")
    trials = []
    for index, sample in enumerate(raw["trials"]):
        # /proc/uptime is truncated to centiseconds. It only locates the tap;
        # the actual input timestamp comes from the delivered MotionEvent.
        start, end = (int(tick * 1e9) for tick in sample["boot"])
        touch = [event for event in events if start - 1_000_000 < int(event["ts"]) < end + 30_000_000]
        if len(touch) != 2 or "ACTION_DOWN" not in touch[0]["name"] or "ACTION_UP" not in touch[1]["name"]:
            raise ValueError(f"Expected one down/up pair for trial {index}")
        down, up = [int(re.search(r"eventTimeNano=(\d+)", event["event"])[1]) for event in touch]
        received = int(touch[1]["ts"])
        handled = received + int(touch[1]["dur"])
        draw = next((value for value in draws if handled <= value < handled + 250_000_000), None)
        if draw is None:
            raise ValueError(f"No candidate draw for trial {index}")
        offset = offset_at(draw)
        frames = [frame for frame in sample["frames"] if frame["PerformTraversalsStart"] <= draw - offset <= frame["SyncQueued"]]
        if len(frames) != 1:
            raise ValueError(f"Ambiguous submitted frame for trial {index}")
        frame = frames[0]
        trials.append(dict(**frame, vsync_id=frame["FrameTimelineVsyncId"], down_ns=down, release_ns=up,
                           received_up_ns=received-offset, handled_up_ns=handled-offset, draw_ns=draw-offset,
                           layer_fences=[pair for pair in sample["fences"] if pair[0] == frame["GpuCompleted"]]))
    return dict(trials=trials, boot_minus_monotonic_start_ns=offset_at(int(events[0]["ts"])),
                boot_minus_monotonic_end_ns=offset_at(int(events[-1]["ts"])))


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
    stage_fields = (
        ("input_delivery_ms", "release_ns", "received_up_ns"),
        ("up_handler_ms", "received_up_ns", "handled_up_ns"),
        ("after_handler_to_draw_ms", "handled_up_ns", "draw_ns"),
        ("draw_to_render_queue_ms", "draw_ns", "SyncQueued"),
        ("render_queue_to_gpu_done_ms", "SyncQueued", "GpuCompleted"),
        ("gpu_done_to_display_ms", "GpuCompleted", "presented_ns"),
    )
    has_stages = "received_up_ns" in trials[0]
    # The traversal boundary splits the main-thread interval; these two figures
    # explain after_handler_to_draw_ms and must not be added to it a second time.
    if has_stages and "PerformTraversalsStart" in trials[0]:
        stage_fields += (
            ("after_handler_to_traversal_ms", "handled_up_ns", "PerformTraversalsStart"),
            ("traversal_to_draw_ms", "PerformTraversalsStart", "draw_ns"),
        )
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
        if has_stages:
            times = {**trial, "presented_ns": shown}
            for field, first, last in stage_fields:
                if times[last] < times[first]:
                    raise ValueError(f"Out-of-order {field} for {trial['vsync_id']}")
                samples[-1][field] = (times[last] - times[first]) / 1e6
    rounds = []
    for start in range(0, len(samples), 65):
        group = samples[start + 5:start + 65]
        summary = {}
        for field in ["down_ms", "release_ms", "dwell_ms"] + ([f for f, _, _ in stage_fields] if has_stages else []):
            values = sorted(sample[field] for sample in group)
            summary[field] = dict(p50=statistics.median(values), p95=values[56], p99=values[59])
        rounds.append(summary)
    return dict(trace_sha256=hashlib.sha256(trace.read_bytes()).hexdigest(), rounds=rounds, samples=samples,
                si_200_300_pass=all(r["down_ms"]["p95"] <= 200 and r["down_ms"]["p99"] <= 300 for r in rounds),
                ak_50_pass=all(r["release_ms"]["p95"] <= 50 for r in rounds),
                down_50_pass=all(r["down_ms"]["p95"] <= 50 for r in rounds))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("trace", type=Path)
    parser.add_argument("processor", type=Path)
    parser.add_argument("--require-ak", action="store_true", help="Exit with failure if any release-to-display P95 exceeds 50 ms")
    args = parser.parse_args()
    processor = args.processor.resolve()
    report = json.loads(args.report.read_bytes())
    if report["trials"] and "frames" in report["trials"][0]:
        report = standalone_input(report, args.trace, processor)
    result = measure(report, args.trace, processor)
    print(json.dumps(result, indent=2))
    if args.require_ak and not result["ak_50_pass"]:
        raise SystemExit(1)
