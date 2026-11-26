"""Measure the standalone release IME against an independent editor on a connected Android device."""

import argparse
import json
import re
import struct
import subprocess
import time
import uuid
from pathlib import Path


def record(serial, package, geometry, output, trials):
    adb = ["adb", "-s", serial]
    host = "org.mywubi.acceptancehost"
    ime = package + "/org.mywubi.MyWubiService"
    if not re.fullmatch(r"[a-zA-Z][\w]*(?:\.[a-zA-Z][\w]*)+", package):
        raise ValueError("Invalid Android package name")

    def shell(command):
        return subprocess.check_output([*adb, "shell", command]).decode().strip()

    def pixels(rect):
        data = subprocess.check_output([*adb, "exec-out", "screencap"])
        width, height, fmt = struct.unpack_from("<III", data)
        header = len(data) - width * height * 4
        if header not in (12, 16) or fmt not in (1, 2) or [width, height] != geometry["display_px"]:
            raise ValueError("Screen geometry or pixel format differs from the coordinate fixture")
        left, top, right, bottom = rect
        return b"".join(data[header + 4*(y*width+left):header + 4*(y*width+right)] for y in range(top, bottom))

    def expect_text(expected):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            row = shell(f"tail -n 1 /sdcard/Android/data/{host}/files/edits.jsonl")
            if row and json.loads(row)["text"] == expected:
                return
            time.sleep(0.05)
        raise ValueError(f"Independent editor did not record {expected!r}")

    def frame_stats():
        lines = shell(f"dumpsys gfxinfo {package} framestats").splitlines()
        header = next(line for line in lines if line.startswith("Flags,FrameTimelineVsyncId,")).rstrip(",").split(",")
        fields = ["FrameTimelineVsyncId", "IntendedVsync", "HandleInputStart", "PerformTraversalsStart",
                  "DrawStart", "SyncQueued", "SwapBuffers", "SwapBuffersCompleted", "GpuCompleted", "FrameCompleted"]
        indices = [header.index(field) for field in fields]
        frames = []
        for line in lines:
            if line[:1].isdigit() and "," in line:
                values = line.rstrip(",").split(",")
                if len(values) == len(header):
                    frames.append(dict(zip(fields, (int(values[i]) for i in indices))))
        return frames[-65:]

    previous = shell("settings get secure default_input_method")
    enabled = ime in shell("ime list -s").splitlines()
    ticket = "ak17-" + uuid.uuid4().hex
    remote = f"/data/misc/perfetto-traces/{ticket}.pftrace"
    output.parent.mkdir(parents=True, exist_ok=True)
    report = {"package": package, "inspection_attached": False, "trials": []}
    recording = False
    try:
        shell(f"am force-stop {package}")
        shell(f"ime enable {ime}")
        shell(f"ime set {ime}")
        shell(f"am force-stop {host}")
        shell(f"am start -W -n {host}/.EditorActivity")
        time.sleep(1)
        left, top, right, bottom = geometry["key_bounds_px"]
        x, y = left + (right-left)//8, (top+bottom)//2
        stroke = f"cmd input touchscreen swipe {x} {y} {x} {y} 40"
        shell(stroke)
        expect_text("p")
        left, top, right, bottom = geometry["candidate_bounds_px"]
        rect = left+6, top+6, right-6, bottom-6
        select = f"input tap {(left+right)//2} {(top+bottom)//2}"
        time.sleep(0.3)
        reference = pixels(rect)

        def commit_and_clear():
            shell(select)
            expect_text("这")
            shell("input keyevent 67")
            expect_text("")

        commit_and_clear()
        time.sleep(0.2)
        if pixels(rect) == reference:
            raise ValueError("Idle and candidate pixels match")
        layers = re.findall(r"(?:^|\{)(InputMethod#\d+)(?: |$)",
                            shell("dumpsys SurfaceFlinger --list"), re.MULTILINE)
        layer, = set(layers)
        report.update(layer=layer, accessibility_enabled=shell("settings get secure accessibility_enabled"),
                      art=shell(f"pm art dump {package}"), geometry={k: geometry[k] for k in
                      ("key_bounds_px", "candidate_bounds_px", "display_px")})
        config = f"""buffers {{ size_kb: 32768 fill_policy: RING_BUFFER }}
data_sources {{ config {{ name: "android.surfaceflinger.frametimeline" }} }}
data_sources {{ config {{ name: "linux.ftrace" ftrace_config {{
    atrace_categories: "view" atrace_categories: "input" atrace_apps: "{package}" }} }} }}
write_into_file: true
file_write_period_ms: 1500
flush_period_ms: 1000
duration_ms: {max(600000, trials*6000)}"""
        subprocess.run([*adb, "shell", "-T", "perfetto", "--txt", "--detach="+ticket, "-c", "-", "-o", remote],
                       input=config.encode(), check=True, capture_output=True)
        recording = True
        for index in range(trials):
            ticks = shell(f"cat /proc/uptime; {stroke}; cat /proc/uptime").splitlines()
            time.sleep(0.25)  # Read pixels only after the normal display frame.
            if pixels(rect) != reference:
                raise ValueError(f"Candidate pixels differ on trial {index+1}")
            frames = frame_stats()
            fences = []
            for line in shell(f"dumpsys SurfaceFlinger --latency {layer}").splitlines():
                if len(values := line.split()) == 3:
                    _, shown, ready = map(int, values)
                    if 0 < shown < 2**63-1:
                        fences.append([ready, shown])
            commit_and_clear()
            if index % 65 in (4, 64):
                meminfo = shell(f"dumpsys meminfo {package}")
                match = re.search(r"TOTAL PSS:\s+(\d+)", meminfo)
                if match:
                    report.setdefault("pss_kib", []).append({"after_trial": index + 1, "pss_kib": int(match[1])})
            report["trials"].append(dict(boot=[float(t.split()[0]) for t in ticks], frames=frames, fences=fences))
            time.sleep(0.12)
    finally:
        try:
            if recording:
                shell(f"perfetto --attach={ticket} --stop")
                subprocess.run([*adb, "pull", remote, str(output.with_suffix(".pftrace"))], check=True)
                shell(f"rm {remote}")
        finally:
            output.write_text(json.dumps(report, ensure_ascii=False), encoding="utf-8")
            shell(f"ime set {previous}")
            if not enabled:
                shell(f"ime disable {ime}")
            shell(f"am force-stop {host}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--package", required=True, help="Isolated, shell-profileable release application ID")
    parser.add_argument("--geometry", required=True, type=Path, help="PhysicalPresentationTest report from the same layout")
    parser.add_argument("--output", required=True, type=Path, help="Raw JSON; the Perfetto trace is saved alongside it")
    parser.add_argument("--trials", type=int, default=195)
    args = parser.parse_args()
    record(args.serial, args.package, json.loads(args.geometry.read_bytes()), args.output, args.trials)
