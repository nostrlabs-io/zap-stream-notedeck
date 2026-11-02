#!/usr/bin/env python3
import argparse
import os
import re
import subprocess
import sys
import tempfile
import time

import numpy as np

ADB_DEFAULT = os.environ.get("ADB_SERIAL", "adb-47021FDAS00035-w5xiL3._adb-tls-connect._tcp")
ADB_BIN = os.path.expanduser("~/Android/Sdk/platform-tools/adb")
BUCKETS = [8.5, 12, 17, 25, 33, 42, 58, 70, 90, 110, 130, 160, 250, 500, 1e9]


def adb(args, serial, **kw):
    return subprocess.run([ADB_BIN, "-s", serial] + args, capture_output=True, text=True, **kw).stdout


def list_layers(serial, pkg):
    out = adb(["shell", "dumpsys", "SurfaceFlinger", "--list"], serial)
    names = []
    for line in out.splitlines():
        m = re.match(r"RequestedLayerState\{(.*?)(#\d+)( |\})", line)
        name = (m[1] + m[2]) if m else line.strip()
        if pkg in name and "ActivityRecord" not in name and "InputSink" not in name:
            names.append(name)
    return names


def poll_latency(serial, layer, secs):
    seen = {}
    end = time.time() + secs
    period = None
    while time.time() < end:
        out = adb(["shell", "dumpsys", "SurfaceFlinger", "--latency", f'"{layer}"'], serial)
        lines = out.split()
        if not lines:
            time.sleep(0.3)
            continue
        period = int(lines[0])
        rows = [lines[i : i + 3] for i in range(1, len(lines) - 2, 3)]
        for d, a, r in rows:
            a = int(a)
            if a > 0 and a < 2**62:
                seen[a] = (int(d), int(r))
        time.sleep(0.35)
    return sorted(seen), period


def histogram(intervals_ms, label, source_ms=None, vsync_ms=None):
    n = len(intervals_ms)
    if n == 0:
        print(f"{label}: no intervals")
        return
    a = np.array(intervals_ms)
    print(f"{label}: intervals={n} mean={a.mean():.1f}ms p50={np.percentile(a,50):.1f} p95={np.percentile(a,95):.1f} p99={np.percentile(a,99):.1f} max={a.max():.0f}")
    lo = 0.0
    for hi in BUCKETS:
        c = int(((a >= lo) & (a < hi)).sum())
        if c:
            name = f"{lo:g}-{hi:g}" if hi < 1e8 else f">={lo:g}"
            print(f"  {name:>10} ms {c:6d} {100*c/n:5.1f}% {'#'*int(60*c/n)}")
        lo = hi
    if source_ms:
        tol = vsync_ms or 8.33
        near = ((a >= source_ms - tol - 1) & (a <= source_ms + tol + 1)).sum()
        beyond = ((a > source_ms + tol + 1) | (a < source_ms - tol - 1)).sum()
        print(f"  within 1 source period +/- 1 vsync ({source_ms:g}ms +/- {tol:g}): {100*near/n:.1f}%  outside: {100*beyond/n:.1f}%")


def cmd_sf(args):
    layers = [args.layer] if args.layer else list_layers(args.serial, args.package)
    if not layers:
        sys.exit("no layers found, is the app in the foreground?")
    print("layers:", *layers, sep="\n  ")
    layer = args.layer or sorted(layers, key=lambda n: ("SurfaceView" not in n, len(n)))[0]
    print("using:", layer, f"for {args.secs}s")
    times, period = poll_latency(args.serial, layer, args.secs)
    iv = [(b - a) / 1e6 for a, b in zip(times, times[1:]) if b - a < 2e9]
    print(f"vsync period {period/1e6 if period else '?'} ms, presented buffers {len(times)}")
    histogram(iv, "SF presented-buffer intervals", args.source_ms, args.vsync_ms)


def cmd_rec(args):
    remote = "/sdcard/present_gaps.mp4"
    local = args.keep or tempfile.mktemp(suffix=".mp4")
    print(f"recording {args.secs}s from the screen")
    subprocess.run(
        [ADB_BIN, "-s", args.serial, "shell", "screenrecord", "--time-limit", str(args.secs), "--size", args.size, "--bit-rate", "20000000", remote],
        check=False,
    )
    subprocess.run([ADB_BIN, "-s", args.serial, "pull", remote, local], check=True, capture_output=True)
    adb(["shell", "rm", remote], args.serial)
    analyse_video(local, args)


def probe_size(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", path],
        capture_output=True, text=True,
    ).stdout.strip()
    w, h = out.split(",")
    return int(w), int(h)


def analyse_video(path, args):
    w, h = probe_size(path)
    if args.crop:
        cx, cy, cw, ch = [int(v) for v in args.crop.split(":")]
    else:
        cw = w
        ch = int(w * 9 / 16)
        cx, cy = 0, int(h * args.top)
    pts = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=pts_time,best_effort_timestamp_time", "-of", "csv=p=0", path],
        capture_output=True, text=True,
    ).stdout.split()
    times = []
    for line in pts:
        parts = [p for p in line.split(",") if p and p != "N/A"]
        times.append(float(parts[0]))
    gw, gh = 96, int(96 * ch / cw)
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-vf", f"crop={cw}:{ch}:{cx}:{cy},scale={gw}:{gh},format=gray", "-fps_mode", "passthrough", "-f", "rawvideo", "-"],
        capture_output=True,
    ).stdout
    frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, gh, gw).astype(np.int16)
    n = min(len(frames), len(times))
    frames, times = frames[:n], times[:n]
    diffs = np.abs(np.diff(frames, axis=0)).mean(axis=(1, 2))
    changed = [times[i + 1] for i in range(len(diffs)) if diffs[i] > args.threshold]
    iv = [(b - a) * 1000 for a, b in zip(changed, changed[1:])]
    print(f"recorded frames {n}, content changes {len(changed)}, crop {cw}x{ch}+{cx}+{cy}, recording fps {n/max(times[-1]-times[0],1e-6):.1f}")
    histogram(iv, "video content-change intervals", args.source_ms, args.vsync_ms)


def locate(path, args):
    w, h = probe_size(path)
    gw, gh = 54, int(54 * h / w)
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-vf", f"scale={gw}:{gh},format=gray", "-fps_mode", "passthrough", "-f", "rawvideo", "-"],
        capture_output=True,
    ).stdout
    frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, gh, gw).astype(np.int16)
    act = (np.abs(np.diff(frames, axis=0)) > 12).mean(axis=0)
    mask = act > 0.5 * act.max()
    ys, xs = np.where(mask)
    x0, x1, y0, y1 = xs.min() * w // gw, (xs.max() + 1) * w // gw, ys.min() * h // gh, (ys.max() + 1) * h // gh
    print(f"recording {w}x{h}, most active region --crop {x0}:{y0}:{x1-x0}:{y1-y0}")
    out = args.out or "locate.png"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", path, "-vframes", "1", "-ss", "3", out])
    print("still saved to", out)


TRACE_CONFIG = """
buffers { size_kb: 32768 fill_policy: RING_BUFFER }
data_sources { config { name: "android.surfaceflinger.frametimeline" } }
duration_ms: %d
"""


def cmd_trace(args):
    remote = "/data/misc/perfetto-traces/present_gaps.pftrace"
    local = args.keep or tempfile.mktemp(suffix=".pftrace")
    cfg = TRACE_CONFIG % (args.secs * 1000)
    print(f"tracing frametimeline for {args.secs}s")
    subprocess.run(
        [ADB_BIN, "-s", args.serial, "shell", "perfetto", "-c", "-", "--txt", "-o", remote],
        input=cfg, text=True, check=False, capture_output=True,
    )
    subprocess.run([ADB_BIN, "-s", args.serial, "pull", remote, local], check=True, capture_output=True)
    adb(["shell", "rm", remote], args.serial)
    analyse_trace(local, args)


def analyse_trace(path, args):
    from perfetto.trace_processor import TraceProcessor

    tp = TraceProcessor(trace=path)
    rows = list(tp.query(
        "select layer_name, ts, dur, present_type, jank_type, on_time_finish from actual_frame_timeline_slice "
        "where layer_name is not null"
    ))
    layers = {}
    for r in rows:
        layers.setdefault(r.layer_name, []).append(r)
    print("layers with presented frames:")
    for name, v in sorted(layers.items(), key=lambda kv: -len(kv[1])):
        print(f"  {len(v):6d} {name}")
    want = [n for n in layers if args.layer and args.layer in n] or [n for n in layers if args.package in n]
    if not want:
        sys.exit("no matching layer, pass --layer with a substring of one above")
    name = sorted(want, key=lambda n: -len(layers[n]))[0] if not args.layer else want[0]
    v = sorted(layers[name], key=lambda r: r.ts)
    print("using layer:", name)
    presents = [r.ts + r.dur for r in v if r.present_type != "Dropped Frame"]
    iv = [(b - a) / 1e6 for a, b in zip(presents, presents[1:])]
    histogram(iv, "SF present-to-present intervals", args.source_ms, args.vsync_ms)
    jank = {}
    for r in v:
        jank[r.jank_type] = jank.get(r.jank_type, 0) + 1
    print("jank types:", jank)


def strip_frames(path, crop, scale):
    cx, cy, cw, ch = [int(v) for v in crop.split(":")]
    pts = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=pts_time", "-of", "csv=p=0", path],
        capture_output=True, text=True,
    ).stdout.split()
    times = [float(t.strip(",")) for t in pts if t.strip(",") not in ("", "N/A")]
    gw, gh = cw * scale, ch * scale
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-vf", f"crop={cw}:{ch}:{cx}:{cy},scale={gw}:{gh}:flags=bicubic,format=gray", "-fps_mode", "passthrough", "-f", "rawvideo", "-"],
        capture_output=True,
    ).stdout
    frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, gh, gw).astype(np.float32)
    n = min(len(frames), len(times))
    return frames[:n], times[:n]


def phase_shift(a, b):
    win = np.hanning(a.shape[1])[None, :] * np.hanning(a.shape[0])[:, None]
    fa = np.fft.fft2((a - a.mean()) * win)
    fb = np.fft.fft2((b - b.mean()) * win)
    r = fa * np.conj(fb)
    r /= np.abs(r) + 1e-9
    c = np.real(np.fft.ifft2(r))
    prof = c.max(axis=0)
    x = int(np.argmax(prof))
    l, m, r = prof[x - 1], prof[x], prof[(x + 1) % len(prof)]
    den = l - 2 * m + r
    sub = 0.5 * (l - r) / den if den != 0 else 0.0
    xf = x + sub
    if xf > a.shape[1] / 2:
        xf -= a.shape[1]
    return -xf, m


def ticker_track(path, args):
    frames, times = strip_frames(path, args.crop or "62:338:135:16", 2)
    ref, ref_pos = frames[0], 0.0
    track = [(times[0], 0.0, 1.0)]
    last = frames[0]
    for i in range(1, len(frames)):
        if np.abs(frames[i] - last).mean() < args.threshold:
            continue
        last = frames[i]
        dx, conf = phase_shift(ref, frames[i])
        pos = ref_pos + dx / 2
        track.append((times[i], pos, conf))
        if abs(dx / 2) > 12:
            ref, ref_pos = frames[i], pos
    return track


def backward(path, args):
    track = [t for t in ticker_track(path, args) if t[2] > args.min_conf]
    if len(track) < 10:
        print("no ticker motion found, check --crop")
        return
    span = track[-1][1] - track[0][1]
    dom = 1.0 if span > 0 else -1.0
    steps = [(b[0], (b[1] - a[1]) * dom) for a, b in zip(track, track[1:])]
    fwd = [s for s in steps if s[1] > args.min_step]
    rev = [s for s in steps if s[1] < -args.min_step]
    dur = track[-1][0] - track[0][0]
    print(f"tracked {len(track)} strip changes over {dur:.0f}s, ticker moved {abs(span):.0f}px ({abs(span)/dur:.1f}px/s) direction {'+x' if dom > 0 else '-x'}")
    print(f"forward steps {len(fwd)}, backward steps {len(rev)} ({100*len(rev)/max(len(fwd)+len(rev),1):.1f}%), backward per 30s {30*len(rev)/dur:.1f}")
    for t, d in rev[:25]:
        print(f"  t={t:7.3f}s back {-d:.2f}px")


def cmd_gfx(args):
    out = adb(["shell", "dumpsys", "gfxinfo", args.package, "framestats"], args.serial)
    rows = []
    for line in out.splitlines():
        if re.match(r"^\d+,", line):
            f = line.split(",")
            if len(f) > 13 and f[0] == "0":
                rows.append((int(f[1]), int(f[13])))
    rows.sort()
    iv = [(b[0] - a[0]) / 1e6 for a, b in zip(rows, rows[1:])]
    dur = [(r[1] - r[0]) / 1e6 for r in rows]
    print(f"UI frames {len(rows)}")
    histogram(iv, "UI frame intents interval", None)
    if dur:
        d = np.array(dur)
        print(f"UI frame total duration p50={np.percentile(d,50):.1f} p95={np.percentile(d,95):.1f} p99={np.percentile(d,99):.1f}")


def main():
    p = argparse.ArgumentParser(description="Histogram of what reaches the display while a stream plays")
    p.add_argument("mode", choices=["sf", "rec", "gfx", "video", "locate", "trace", "tracefile", "backward"])
    p.add_argument("--serial", default=ADB_DEFAULT)
    p.add_argument("--package", default="stream.zap.app")
    p.add_argument("--layer")
    p.add_argument("--secs", type=int, default=30)
    p.add_argument("--source-ms", type=float, default=50.0)
    p.add_argument("--vsync-ms", type=float, default=8.33)
    p.add_argument("--size", default="504x1122")
    p.add_argument("--top", type=float, default=0.07)
    p.add_argument("--crop")
    p.add_argument("--threshold", type=float, default=0.15)
    p.add_argument("--out")
    p.add_argument("--min-step", type=float, default=0.5)
    p.add_argument("--min-conf", type=float, default=0.3)
    p.add_argument("--keep")
    p.add_argument("--file")
    a = p.parse_args()
    if a.mode == "sf":
        cmd_sf(a)
    elif a.mode == "rec":
        cmd_rec(a)
    elif a.mode == "gfx":
        cmd_gfx(a)
    elif a.mode == "trace":
        cmd_trace(a)
    elif a.mode == "tracefile":
        analyse_trace(a.file, a)
    elif a.mode == "backward":
        backward(a.file, a)
    elif a.mode == "locate":
        locate(a.file, a)
    else:
        analyse_video(a.file, a)


if __name__ == "__main__":
    main()
