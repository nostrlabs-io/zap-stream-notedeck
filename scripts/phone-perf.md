# Phone performance check (Pixel 9 Pro XL)

The supervisor runs this on the user's phone. It takes about 5 minutes.

## What to install

The release APK from the dry run of the release workflow, or a local build:

```
gh run download <run id> --repo nostrlabs-io/zap-stream-notedeck -n android -D /tmp/zs
P=adb-47021FDAS00035-w5xiL3._adb-tls-connect._tcp
adb -s $P install -r /tmp/zs/zap_stream-v1.3.0-android-arm64.apk
```

It is signed with the Nostr Labs upload key, the same as the Play build of the Flutter app, and its
version code (1030000) is above the Flutter release (2024), so it installs over the Flutter app as
an update. The Flutter app's own login and settings do not carry over.

## Before

```
adb -s $P logcat -c
adb -s $P shell dumpsys gfxinfo io.nostrlabs.zap_stream_flutter reset
adb -s $P shell am start -W -n io.nostrlabs.zap_stream_flutter/stream.zap.app.MainActivity
```

`TotalTime` from `am start -W` is the cold start time.

## The 3 minutes

1. 0:00 to 0:30: stay on the home page, scroll down and back up twice.
2. 0:30: open NoGood Radio (or the stream at the top of the home page) with Watch stream.
3. 0:30 to 2:00: let it play, scroll the chat up a screen and back down every 20 seconds.
4. 2:00 to 2:30: rotate to landscape fullscreen, back to portrait.
5. 2:30: press Home, wait 20 seconds (audio keeps playing, the media controls are in the shade),
   open the app again.
6. 3:00: stop.

## Read the result

```
adb -s $P logcat -d -s zapstream:I > /tmp/zs-phone.log
grep -E "first_frame|frame_stats|work_stats|slow_section" /tmp/zs-phone.log
adb -s $P shell dumpsys meminfo io.nostrlabs.zap_stream_flutter | grep -E "TOTAL PSS|TOTAL RSS"
adb -s $P shell dumpsys media_session | grep -A12 "zap.stream io.nostr" | grep -E "state=|metadata"
python3 scripts/frame_gaps.py /tmp/zs-phone.log
```

- `frame_stats` is the time between frames; `work_stats` is the CPU time the app spends in one
  frame. Targets from #98: p95 under 12 ms with chat scrolling and video, no `max` over 50 ms
  outside of page changes.
- `scripts/frame_gaps.py` gives the video frame gap histogram (#105, #108).
- Paste the `frame_stats` and `work_stats` lines of the video part (0:30 to 2:00) and the
  meminfo lines into docs/PERF.md under "Pixel 9 Pro XL".
