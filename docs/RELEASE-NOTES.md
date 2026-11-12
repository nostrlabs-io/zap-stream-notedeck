# Store release notes

## What's new (Play and App Store, 1.3.1)

zap.stream is now a native app, rebuilt from the ground up. It opens faster, scrolls smoother and
plays live streams with hardware decoding.

- Watch live, planned and ended streams, with chat, zaps, reactions and custom emoji.
- Go live alerts for streamers you follow, background audio and picture in picture on Android.
- Log in with a key, Amber or a NIP-46 bunker, or just browse without an account.
- Report any stream, message or profile, block users, and hide streams with a content warning.
- Available in 22 languages.

## App Review note (Apple) and Play review note

zap.stream is a viewer for live streams published on the open Nostr protocol. No account is
needed to review it: the home page, stream pages, chat, profiles, search and categories all work
logged out. Chat and zaps are read only until a key is added. To test the signed in features, use
Log in and choose "Create account", which makes a new key on the device in one tap.

User generated content (guideline 1.2):

- Terms of service and the content policy are shown on first launch and must be accepted before
  any stream or chat is shown. They stay readable in Settings, Terms of service.
- Every stream, chat message and profile has a Report button (reason list, published as a NIP-56
  report to the relays), and every user can be blocked or muted. Blocked users' streams and chat
  messages disappear at once.
- Streams that carry a content warning tag are hidden by default. Settings, Sensitive content
  turns them on.
- Developer contact: info@zap.stream, shown in Settings, Contact the developer, and on the terms
  screen. Reports are reviewed by the zap.stream team.

Account deletion (guideline 5.1.1(v)): Settings, Delete account. It blanks the profile, publishes
a deletion request for the user's events and removes the key from the device. Nostr has no central
server, so relays may keep copies, and the confirmation screen says so.

Payments (guidelines 3.1.1 and 3.1.3): zaps are voluntary tips sent to creators over the Lightning
network, paid from the user's own external wallet (Nostr Wallet Connect, or an invoice paid in
another wallet app). The app does not sell anything, has no in-app purchases, subscriptions or
credits, and a zap never unlocks content or features: every stream is free to watch with or
without a zap. The app takes no cut. Tips are person to person and do not go through the app
developer.

Camera, microphone and location are not used. The app has no analytics or ads.
Export compliance: only standard HTTPS and Nostr signatures, ITSAppUsesNonExemptEncryption is
false.

## Play Console answers

- Target API level: 36 (required for updates from 31 August 2026).
- Content rating questionnaire: user generated content, live streams shared between users.
- Data safety: collects user content (chat messages, profile) and identifiers (public key) which
  are published by the user to Nostr relays; no data sold, none shared with third parties by the
  developer; no ads, no analytics. Data in transit is encrypted (wss and https). Deletion is
  requested in the app (Settings, Delete account).
- Account deletion URL: a public web page describing the in-app steps is required in the Data
  safety form (see the tracker issue for the page).
- Foreground service type: media playback (background audio).
- Notifications permission: go live alerts.
