# Store screenshots

One command makes the Play screenshots from the native app:

```bash
scripts/store-shots.sh
```

Options: `--skip-build` reuses the debug APK, `--no-captions` skips the captioned set, `--only <scene>` retakes one scene (home, category, categories, search, profile, language, settings). Set `STORE_SHOTS_VIDEO=1` to add the stream page scenes (stream with chat, chat with zaps, landscape player); see "Video scenes" below.

## What it does

1. Builds an x86_64 debug-style APK of the app with the `store-shots` cargo feature (`cargo ndk ... --release --lib --features store-shots`, then `gradlew :app:assembleDebug`). The feature adds a hook that reads `files/settings/pending_page` from the app data dir to open a page. No release script, CI job or the AAB build enables it, so the shipped app has no such hook.
2. Creates three AVDs in a private `ANDROID_AVD_HOME` (`~/.cache/zap-store-avd`): Pixel 7 phone (1080x1920 at 420 dpi), Nexus 7 tablet (1200x1920) and Nexus 10 tablet (1600x2560), Android 34 Google APIs x86_64. They boot one at a time, headless (`-no-window -no-audio -no-snapshot -gpu swiftshader_indirect`) on port 5590 (`STORE_SHOTS_PORT`). No other emulator is touched.
3. Turns on SystemUI demo mode (9:41, full battery, full wifi, no notifications) and turns it off on exit.
4. Accepts the terms by writing the app's `terms_accepted` file, then drives the app. Pages are opened with the hook above and `nostr:` links. Waits find text by screenshot OCR (`scripts/store_shots_ui.py`, tesseract), because the app draws with egui and `uiautomator dump` shows only one empty view.
5. `scripts/store_shots_pick.py` picks a live, public stream from `relay.zap.stream` (most viewers first, skipping content warnings and adult words). If none is live it falls back to the most watched ended stream and the report must say so. The choice is saved in `store/play/<set>/stream.json`.
6. Captions (`scripts/store_shots_captions.py`) are drawn with the app's Bricolage font and colours into `store/play-captioned/`.
7. `scripts/store_shots_check.py` runs last and fails on wrong count, size, aspect, file size, black or white frames, alpha, or two identical shots.

## Play Console requirements

From the Play Console help page "Add preview assets to showcase your app" (checked 23 Nov):

| Item | Count | Size | Format |
|---|---|---|---|
| Phone | 2 to 8 | each side 320 to 3840 px, long side at most 2.3 times the short side, 9:16 or 16:9 | PNG or JPEG, no alpha, 8 MB max |
| 7 inch tablet | 4 or more to be featured on large screens | 1080 to 7680 px, 9:16 or 16:9 | same |
| 10 inch tablet | 4 or more | same | same |
| Feature graphic | 1 | 1024x500 | JPEG or 24 bit PNG |

## Video scenes

The emulator's H.264 decoder cannot show a real picture (black or tinted frames), so by default the script skips the stream page scenes rather than ship a broken player. Run `STORE_SHOTS_VIDEO=1 scripts/store-shots.sh` against a device or emulator with a working decoder to add them.

## Apple

App Store Connect needs iPhone 6.9 inch (1320x2868) and iPad 13 inch (2064x2752) shots in `store/appstore/iphone-6.9/` and `store/appstore/ipad-13/`; the check script already validates those folders. They are made on the Mac simulator (`ios.sh`, `xcrun simctl io booted screenshot`, `SIMCTL_CHILD_ZAP_STREAM_PAGE` and `SIMCTL_CHILD_ZAP_STREAM_OPEN` to pick the page). The Mac login keychain must be unlocked first, and nothing may open a GUI window there.

## Uploading to Play (done by a person, never by the script)

1. Play Console, Grow, Store presence, Main store listing.
2. Phone screenshots, upload the files from `store/play/phone/` (or `store/play-captioned/phone/`) in file name order. Delete the old Flutter screenshots first.
3. 7 inch tablet and 10 inch tablet screenshots, same with `tablet-7` and `tablet-10`.
4. Save, then send for review with the next release.
