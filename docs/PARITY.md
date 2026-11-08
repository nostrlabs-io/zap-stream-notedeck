# Feature parity with the Flutter app

Compared against nostrlabs-io/zap-stream-flutter at d4122aa (v1.2.3+24): `lib/pages`, `lib/widgets`
and `lib/*.dart`. "Rust" names the file that implements it and the tests that cover it, where there
are any. Every gap has an issue in the agentbench tracker.

| Area | Flutter | Rust | Tests | State |
|---|---|---|---|---|
| Home: live, planned, ended sections | `pages/home.dart`, `widgets/stream_grid.dart` | `src/route/home.rs`, `src/widgets/stream_list.rs` | `route::home` pull to refresh | done |
| Home: following section | `stream_grid.dart` (follows first) | `src/route/home.rs`, `src/contacts.rs` | `contacts` | done |
| Home: pull to refresh | `RefreshIndicator` | `src/route/home.rs` `PullToRefresh` | 4 tests in `route::home` | done |
| Most zapped streamers | `category_top_zapped.dart` (home) | `src/route/home.rs` zapped panel, `src/zap.rs` | `zap` | done |
| Stream tile: poster, live pill, viewers, host | `stream_tile.dart`, `pill.dart` | `src/widgets/stream_tile.rs`, `src/poster/*` | `poster::chain`, `poster::cache` | done |
| Header: logo, search, login, go live | `widgets/header.dart` | `src/widgets/header.rs`, `src/widgets/sidebar.rs` | | done |
| Search: streams, profiles, hashtags | `pages/search.dart` | `src/route/search.rs` | `route::search` | done |
| Hashtag page `/t/:id` | `pages/hashtag.dart` | `src/route/listing.rs` `Listing::Hashtag` | | done |
| Category page `/category/:id` grid | `pages/category.dart` | `src/route/listing.rs` `Listing::Category`, `CategoriesPage` | | done |
| Category header: cover, genres, top zapped | `category.dart`, `game_info.dart`, `category_top_zapped.dart` | none | | gap: #345 |
| Stream page: player, chat, info sheet | `pages/stream.dart`, `stream_info.dart` | `src/route/stream.rs`, `src/widgets/stream_title.rs` | | done |
| Stream info: summary, tags as pills, share, started, live timer | `stream_info.dart`, `live_timer.dart` | `src/widgets/stream_title.rs`, `src/widgets/player_controls.rs` | `stream_info` | done |
| Game tag label | `game_info.dart`, `utils.dart extractGameTag` | `src/stream_info.rs` `is_game_tag`, `game_label` | `stream_info` | done |
| Stream cards (kind 17777 / 37777) | `stream_cards.dart` | `src/widgets/stream_cards.rs` | `stream_cards` | done, card text is plain (no markdown) |
| Planned countdown, ended and offline cards | `countdown.dart`, `stream.dart` | `src/widgets/stream_player.rs` `paint_state_card`, `src/player_health.rs` | `player_health` | done |
| Video: play/pause, mute, volume | `video_player_main.dart`, `mute_button.dart` | `src/widgets/player_controls.rs`, `src/widgets/stream_player.rs` | `player_controls` | done |
| Video: quality selector | `stream.dart _QualitySelector` | `src/widgets/player_controls.rs`, `src/hls.rs` | `hls` | done |
| Video: fullscreen and landscape | `video_player_main.dart` | `src/widgets/stream_player.rs`, `src/android_window.rs`, `MainActivity.kt` | `android_window`, `TouchMappingTest.kt` | done |
| Video: Android PiP | `player.dart` | `MainActivity.kt enterPip` | | done |
| Video: reconnect on stall | `player.dart` | `src/player_health.rs` | `player_health` | done |
| Video: background playback, lock screen controls | `player.dart allowBackgroundPlayback`, audio service | none | | gap: #64 |
| Keep screen on while watching | `wakelock_plus` | `MainActivity.kt FLAG_KEEP_SCREEN_ON` | | done |
| Chat: messages, zaps, raids, timeouts, badges | `chat*.dart` | `src/widgets/chat*.rs`, `src/chat_data.rs`, `src/chat_model.rs` | `chat_model` (17), `chat_data`, `chat_events` | done |
| Chat: reactions and emoji picker | `chat_reactions.dart`, `emoji.dart` | `src/widgets/chat_reactions.rs`, `src/widgets/emoji_picker.rs` | `widgets::emoji_picker` | done |
| Chat: NIP-30 custom emoji | `custom_emoji.dart` | `src/chat_text.rs` | `chat_text` | done |
| Chat: nostr: mentions and note embeds | `nostr_text.dart`, `note_embed.dart` | `src/chat_text.rs`, `src/widgets/note_embed.rs`, `src/note_ref.rs` | `chat_text` | done |
| Chat modal: react, timeout, mute | `chat_modal.dart` | `src/widgets/chat.rs render_modal` | | done |
| Chat modal: zap the message | `chat_modal.dart` bolt button | none | | gap: #343 |
| Chat write: @mention and :emoji: autocomplete | `chat_write.dart` | `src/widgets/write_chat.rs` (picker button only) | | gap: #342 |
| Chat write: disabled for npub login and logged out | `chat_write.dart` | `src/widgets/write_chat.rs` | | done |
| Chat top zappers row | `chat.dart` | `src/widgets/chat.rs top_zappers_row` | | done |
| Zap modal: amounts, custom, comment, NWC pay, invoice QR, open wallet | `widgets/zap.dart` | `src/widgets/zap.rs`, `src/zap.rs` | `zap` | done |
| Stream goal (kind 9041) | `goal.dart` | `src/widgets/goal.rs` | `widgets::goal` | done |
| Profile page: avatar, name, about, past streams | `pages/profile.dart` | `src/route/profile.rs` | | done |
| Follow / unfollow (kind 3) | `button_follow.dart` | `src/contacts.rs`, `src/route/profile.rs` | `contacts` | done |
| Mute user (kind 10000) | `mute_button.dart` | `src/widgets/mute_button.rs`, `src/moderation.rs` | `moderation` | done |
| Report and block (App Store review) | none | `src/widgets/report.rs`, `src/moderation.rs` | `moderation` | Rust only |
| Go-live notification bell per streamer | `notifications_button.dart`, `notifications.dart` | none | | gap: #64 |
| Login: nsec, npub (read only) | `pages/login_input.dart`, `login.dart` | `src/route/login.rs parse_key` | `route::login` | done |
| Login: Amber (NIP-55) | `pages/login.dart` | `src/signer.rs`, `Nip55Signer.kt` | `signer` (21) | done |
| Login: NIP-46 bunker:// | `login_input.dart` | none | | gap: #341 |
| Create account, name, avatar upload | `pages/new_account.dart`, `avatar_upload.dart` | `src/route/login.rs`, `src/blossom.rs` | `blossom`, `route::login` | done |
| Settings: edit profile (kind 0) | `pages/settings_profile.dart` | `src/route/settings.rs`, `src/profile_edit.rs` | `profile_edit` | done |
| Settings: wallet, paste NWC, balance, disconnect | `pages/settings_wallet.dart` | `src/route/settings.rs wallet` | | done |
| Settings: NWC 1-tap connect (nostr+walletauth) | `settings_wallet.dart _start1TapFlow` | none | | gap: #340 |
| Settings: relays | none (fixed list) | `src/route/settings.rs`, `src/relay_list.rs` | `relay_list` | Rust only |
| Settings: delete account | none | `src/route/settings.rs` | | Rust only |
| Logout, multiple accounts | `pages/profile.dart` | `src/route/mod.rs Logout`, `SelectAccount` | | done |
| Go live: endpoint, rate, balance, TOS, title, summary, tags, NSFW, goal | `widgets/stream_config.dart`, `api.dart` | `src/route/settings/broadcast.rs`, `src/zap_api.rs` | `zap_api` | done |
| Go live: RTMP/SRT URL and key | `stream_config.dart` | `src/route/settings/broadcast.rs` | | done |
| Go live: camera capture and publish | `pages/live.dart` (apivideo_live_stream) | none | | gap: #339 (needs a user decision) |
| Deep links: zap.stream/naddr, nostr: | `app.dart` routes, intent filters | `src/deep_link.rs`, `AndroidManifest.xml` | `deep_link` | done |
| Relay health banner | `relay_health.dart` | `src/relay_health.rs`, `src/widgets/relay_banner.rs` | `relay_health` | done |
| i18n: 15 locales (cs da de en es fi fr hu it nl pl pt ro sv tr) | `lib/i18n` | `assets/i18n/*.tsv`, `src/i18n.rs` | `i18n` | done |
| i18n: ar el ja ko ru uk zh | `lib/i18n` | none | | gap: #346 |
| Language picker | system locale only | `src/widgets/header.rs` | | Rust only |

## Gaps

- #64 go-live push notifications, background audio and lock screen controls.
- #339 camera broadcast. Needs a decision on the capture stack, see the issue.
- #340 NWC 1-tap connect.
- #341 NIP-46 bunker login.
- #342 chat @mention and :emoji: autocomplete.
- #343 zap a chat message.
- Stream card text is plain, Flutter renders markdown. No markdown renderer in the repo (egui_commonmark not in Cargo.lock).
- #345 category page header.
- #346 seven more locales.
