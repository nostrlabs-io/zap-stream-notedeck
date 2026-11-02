# present_gaps.py

Measures what reaches the display while a stream plays. Keep the app in the foreground on the stream page for the whole run, screen unlocked, 30s.

Ground truth, works for both apps (screen recording, no root):

```
python3 scripts/present_gaps.py rec --secs 30 --source-ms 50
```

Prints the histogram of intervals between visible video content changes. Crop is the top 16:9 area under the status bar; override with `--crop x:y:w:h` (pixels of the recorded size). The recording is capped at 60 fps by screenrecord, so values are quantised to 16.7ms.

SurfaceFlinger presented buffer times (needs the app layer name):

```
python3 scripts/present_gaps.py sf --package stream.zap.app --secs 30
python3 scripts/present_gaps.py sf --package io.nostrlabs.zap_stream_flutter --secs 30
```

The default picks a SurfaceView layer if one exists, else the shortest matching layer. For our app before the SurfaceView change this shows UI frames (egui redraw cadence), not video frames.

`gfx` mode prints UI frame intervals from `dumpsys gfxinfo <pkg> framestats`. `video --file x.mp4` re-analyses a saved recording.

`locate --file x.mp4` prints the most active region of a recording as a `--crop x:y:w:h` value.

Screenrecord only emits a frame when the screen changes. An app that redraws at 120Hz is captured at about 120 fps, a player that only updates at the video rate (Flutter, SurfaceView) is captured at about the video rate, so its timestamps are the composition times. Both are valid, use `--keep file.mp4` to save the recording. For a SurfaceView app, `sf` mode on the video layer (name from `dumpsys SurfaceFlinger --list | grep SurfaceView`, pass with `--layer`) is the more exact measure.

`backward --file x.mp4` tracks the NoGood ticker (bottom left of the video, default crop for the 504x1122 recording) by phase correlation against a moving reference and counts steps against the scroll direction. Flutter control recording: 0 per 30s.

`trace` records a perfetto `android.surfaceflinger.frametimeline` trace and histograms present times per layer (needs `pip install perfetto`).
