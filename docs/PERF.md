# Performance

Measured for 1.3.0 on 2026-11-08 (#338). Two numbers per run:

- `frame_stats`: time between frames. On desktop this is paced by vsync (60 Hz in the headless
  sway, so 16.0 ms is the floor), so it shows stalls, not cost.
- `work_stats`: CPU time the app spends in one frame (`ZapStreamApp::render`, start to end). This
  is the number to compare with the #98 target of p95 under 12 ms.

p95(mean) is the mean of the 5 s window p95 values, p95(worst) the worst window. The first two
windows (start-up) are left out.

## Desktop (Linux, Ryzen, release build, headless sway)

`scripts/desktop-perf.sh <out> <secs> <WxH> [ENV=...]`. Chat scrolling is driven by
`ZAP_STREAM_DEBUG_SCROLL=px,period,x,y` (wheel events over the chat, alternating direction every
period seconds). The stream is Cyberspace TV on api-uk.zap.stream (HLS, video playing through
lumina-video and GStreamer).

| Page | Size | Frames/5 s | frame p50 | frame p95 mean / worst | frame max | work p50 | work p95 mean / worst | work max |
|---|---|---|---|---|---|---|---|---|
| Home, scrolling | 400x850 | 313 | 16.02 | 16.10 / 16.11 | 28.6 | 0.49 | 0.68 / 0.76 | 1.75 |
| Home, scrolling | 1400x900 | 313 | 16.02 | 16.08 / 16.10 | 27.3 | 0.49 | 0.67 / 0.71 | 0.93 |
| Stream, video, chat scrolling | 400x850 | 312 | 16.03 | 16.08 / 16.10 | 28.3 | 0.83 | 1.28 / 1.50 | 2.80 |
| Stream, video, chat scrolling | 1400x900 | 312 | 16.02 | 16.09 / 16.11 | 24.2 | 0.89 | 1.42 / 1.50 | 4.40 |
| Stream, video, chat scrolling, 10 min | 400x850 | 312 | 16.02 | 16.13 / 18.64 | 27.8 | 0.78 | 1.23 / 1.59 | 13.2 |

- Target p95 under 12 ms with chat scrolling and video: met (work p95 1.6 ms worst window).
- No stall over 50 ms: met (worst frame interval 28.6 ms after start-up). The one 40 ms frame is
  the player being created when the stream page opens.
- Start-up to first frame: 373-532 ms (`first_frame` log line, from `main`).
- Memory (RSS): home 285-300 MB; stream page 693-729 MB one minute in, 522 MB after 10 minutes
  of video and chat, so no growth over time.

Before (#317, after the lumina-video upgrade): frame interval p50/p95/p99 about 16 ms steady on the
live page, no work time measured.

## Android emulator (x86_64 Pixel AVD, swiftshader, no video decode)

The emulator renders with a software GPU and its goldfish H.264 decoder cannot play streams, so
frame intervals here are the software renderer, not the app. The CPU work per frame is
comparable.

| Page | Frames/5 s | frame p50 | frame p95 mean / worst | work p50 | work p95 mean / worst | work max |
|---|---|---|---|---|---|---|
| Home, swiping | 48 | 45.5 | 479 / 1210 | 0.94 | 1.12 / 1.16 | 1.73 |
| Stream page, chat swiping (no video) | 103 | 63.2 | 95 / 159 | 0.66 | 2.31 / 5.62 | 11.0 |

- Cold start (`am start -W` TotalTime): 715-917 ms; first frame 199 ms after `android_main`.
- PSS on the stream page: 155 MB.
- The signed arm64 release APK also runs on the emulator under arm64 translation (home page,
  frame p50 46 ms).

## Pixel 9 Pro XL

Not measured for 1.3.0 yet: the procedure is `scripts/phone-perf.md`. Numbers before the lumina
upgrade (#98, #108): p50 7.3 ms, p95 20-50 ms, p99 360-500 ms stalls, chat 8-10 ms per frame,
leaderboard spikes to 76 ms.
