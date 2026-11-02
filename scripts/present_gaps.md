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
