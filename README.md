# zap.stream

A native [zap.stream](https://zap.stream) app built with [notedeck](https://github.com/damus-io/notedeck).

This is **ALPHA** stage software, use at your own risk!

![preview](https://nostr.download/d3ceb5b379d7faddd9d6c536292bf46e097b35fbcaf431ff0a1cbd3e3acaf052.webp)
![preview_mobile](https://nostr.download/7d303cac1b4c923e76f386b3008de5551c0a249d9ceeddafb424dfe4a34ae533.webp)


## Building

Video plays through [lumina-video](https://github.com/v0l/lumina-video) (hardware decode, zero-copy GPU frames).

- Linux: `./debian.sh` (GStreamer with VA-API for zero-copy)
- macOS / iOS: `cargo build --release` (needs `brew install ffmpeg cmake`)
- Android: `./android.sh` (cargo-ndk, JDK 21, NDK 28)

Set `ZAP_STREAM_FRAME_STATS=1` to log frame times every 5 seconds (always on for Android). `scripts/desktop-perf.sh` runs the app in a headless sway and summarises them; see `docs/PERF.md`.

## Notifications and background audio

The bell on a stream or profile turns on go live alerts for that streamer. The list lives in
`notify.txt` in the settings dir. On Android a WorkManager job runs every 15 minutes, asks the
user's relays (kind 10002, or the default relays) for live kind 30311 events of the watched
streamers and posts one notification per stream, which opens the stream when tapped. The Flutter
app used FCM through notepush.nostrlabs.io, which is offline (#355); push can be added back on top
of the same list.

While a stream plays, Android runs a foreground media service with a MediaSession (lock screen and
shade controls, audio keeps playing in the background). iOS sets the audio session category to
playback and declares the `audio` background mode.

## Releasing

The version lives in `Cargo.toml` only. Android `versionName` and iOS `CFBundleShortVersionString`
are read from it, and the build number is `major*1000000 + minor*10000 + patch*100` (1.3.1 is
1030100), above the Flutter app's split APK codes (2024 for arm64 on 1.2.3).

1. Bump `version` in `Cargo.toml`, run `cargo check` so `Cargo.lock` follows, and add a
   `## [x.y.z]` section to `CHANGELOG.md`.
2. Dry run: `gh workflow run release.yml --repo nostrlabs-io/zap-stream-notedeck -f tag=vx.y.z`.
   It builds and uploads artifacts but publishes nothing.
3. Push the tag `vx.y.z`. `.github/workflows/release.yml` checks that the tag matches `Cargo.toml`
   and the changelog has an entry, then builds the Linux x86_64, macOS arm64 and Windows x64
   binaries, the signed arm64 APK and the signed AAB, runs the tests, and publishes a GitHub release
   with the changelog section and `SHA256SUMS`.
4. iOS: `scripts/release-apple.sh --attach` builds the signed IPA on `ssh mac` (identity and
   profile in `~/kgdist` there), checks its version and build number, and attaches it with its
   SHA-256 to the release. Upload to App Store Connect is a separate manual step.

Android signing uses the Nostr Labs upload key. The application id is `io.nostrlabs.zap_stream_flutter`,
the same as the Flutter app, so the AAB ships as an update to the existing Play listing. CI reads
the secrets `KEYSTORE` (base64 of the PKCS12 file), `KEYSTORE_SHA256`, `KEYSTORE_PASSWORD`,
`KEY_ALIAS` and `KEY_PASSWORD`, the same names as the Flutter repo. Locally:
`KEYSTORE_FILE=... KEYSTORE_PASSWORD=... KEY_ALIAS=upload scripts/release.sh` writes the APK, the AAB
and their sums to `dist/`.

The web build is not produced: notedeck's `enostr` relay transport depends on `mio` sockets, so
`cargo check --target wasm32-unknown-unknown` fails in `mio` before reaching this crate.
