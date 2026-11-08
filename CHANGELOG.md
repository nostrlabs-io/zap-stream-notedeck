# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.3.0] - 2026-11-08

The first release of the native Rust app. It replaces the Flutter app (last release 1.2.3) under the
same Android application id and iOS bundle id. Feature status against the Flutter app is in
`docs/PARITY.md`.

### Added

- Home page with live, planned and ended streams, the following section, pull to refresh and the
  zapped in the last 24 hours panel, paged past the relay 500 event cap.
- Stream page with video through lumina-video (hardware decode, zero-copy frames on Android, iOS,
  macOS and Linux), player controls, quality picker, fullscreen and landscape, Android
  picture-in-picture, stall recovery and planned, ended and offline cards.
- Poster fallback chain for stream tiles with a cached frame grabbed from the live HLS stream.
- Chat with zaps, reactions, emoji picker, NIP-30 custom emoji, badges, raids, timeouts, mentions,
  note embeds and a top zappers row.
- Zaps over NWC or a lightning invoice with QR code, and stream goals.
- Profiles with follow, mute, report and block, past streams, followers, nip05 and lightning address.
- Login with nsec, npub (read only) and Amber (NIP-55), account creation with avatar upload over
  Blossom, several accounts.
- Settings for profile, NWC wallet, relays, delete account and the go live page (ingest URL,
  stream key, title, summary, tags, goal and content warning through the zap.stream API).
- Category, hashtag and search pages.
- Deep links for zap.stream URLs and nostr: URIs.
- Relay health banner.
- String tables in 15 languages and a language picker.
- Android soft keyboard insets, clipboard paste, edge to edge header around the camera cutout.
- iOS app bundle, App Store packaging, safe area insets.
- Frame time statistics with `ZAP_STREAM_FRAME_STATS=1`, including CPU work per frame and the time
  to the first frame.
- Go live alerts: a bell per streamer, checked on the user's relays every 15 minutes on Android,
  with a notification that opens the stream.
- Background audio with lock screen and notification controls on Android, playback audio session
  and background audio mode on iOS.
- NIP-46 bunker login, NWC 1-tap wallet connect, chat mention and emoji autocomplete, zapping a
  chat message, stream cards, category page header, and Greek, Japanese, Korean, Russian,
  Ukrainian and Chinese strings.
- Tag driven release workflow building the Android APK and AAB, desktop builds and SHA-256 sums.

[Unreleased]: https://github.com/nostrlabs-io/zap-stream-notedeck/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/nostrlabs-io/zap-stream-notedeck/releases/tag/v1.3.0
