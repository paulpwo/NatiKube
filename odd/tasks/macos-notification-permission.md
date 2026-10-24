# macOS notification permission

## Problem
OS notifications on macOS go through `notify-rust` → `mac-notification-sys`, which uses `NSUserNotificationCenter` (deprecated since macOS 11). It never asks for permission, NatiKube never appears in System Settings → Notifications, and on recent macOS notifications may be dropped silently. Paul (2026-10-24) noticed that turning on OS notifications never asked for permission.

## Objective
On macOS, send notifications through `UNUserNotificationCenter`, ask for permission when the user turns OS notifications on, and show the permission state in Settings → Alerts. Windows keeps its current toast backend.

## Scope
- macOS adapter in `crates/features/alerts/src/adapters/notify/` built on `objc2-user-notifications` (already in Cargo.lock through GPUI; same major version), replacing `notify-rust` on macOS only. `notify-rust` stays for Windows (target-specific dependency).
- `requestAuthorizationWithOptions(alert | sound)` runs when the user turns "OS notifications" on (and on startup only if the setting is already on and the status is not determined). Never at startup otherwise.
- Permission state (not determined / allowed / denied / unavailable) shown under the toggle; when denied, an "Open System Settings" button opens `x-apple.systempreferences:com.apple.Notifications-Settings.extension` (fallback: the Notifications pane URL).
- When the binary does not run from a `.app` bundle (no bundle identifier), the UN API is unavailable: show "Available when NatiKube runs as an installed app" and send nothing (no crash).
- "Send test notification" uses the same path and reports the result in the UI.

## Risk
Ad-hoc signed apps (no Apple Developer ID) may not be allowed to use `UNUserNotificationCenter`. Verify on this Mac with the bundle built by `scripts/bundle.sh`. If macOS refuses, stop and report the exact error instead of working around it.

## Tasks
- [x] N1 macOS UN adapter: authorization request, status query, deliver notification, bundle detection; Windows unchanged; cfg-gated deps.
- [x] N2 Settings → Alerts: permission state line, request on enable, "Open System Settings" when denied, test notification result.
- [ ] N3 Verify with the ad-hoc signed `.app` bundle on this Mac: authorization callback result and app listed in System Settings → Notifications; docs (README alerts section, architecture.md).
- [ ] N4 In-app toasts (Paul 2026-10-24): when an alert fires, a toast in a corner of the main window shows severity, title, cluster and object; it auto-dismisses after a few seconds (hover keeps it), stacks up to 3, and a click navigates to the object like the bell. Independent of the macOS permission. Setting "In-app toasts" (default on; alerts themselves stay opt-in per rule). Styled for all four appearances.
- [ ] N5 Optional alert sound (Paul 2026-10-24): Settings → Alerts gets "Play a sound" (default off), a sound picker of system sounds (macOS: NSSound named sounds such as Glass, Ping, Submarine, Funk, Hero, Pop, Purr, Sosumi, Tink, Basso, Blow, Bottle, Frog, Morse; Windows: system sound aliases such as SystemAsterisk, SystemExclamation, SystemHand, SystemNotification, SystemDefault), a Preview button, and "Only critical alerts" (default off). Native APIs only (objc2-app-kit NSSound already in the lock; Windows PlaySoundW via a crate already in the lock or windows-sys), no audio crate. Sound plays once per batch of alerts, not once per alert, and never more than once per 2 s.
- [ ] N6 Follow-ups from validation: show the exact NSError of a refused authorization in Settings → Alerts (R1); move the macOS status round-trip in `show` off the shared Tokio worker (spawn_blocking or cached status).

## Acceptance
- Turning OS notifications on in the installed app shows the macOS permission prompt once.
- With permission granted, the test notification appears as NatiKube.
- With permission denied, Settings says so and offers to open System Settings.
- fmt, clippy -D warnings, workspace tests green; CI green on Windows.

## Route
Delegated direct: one Sonnet writer (adapter + settings UI + shell wiring), Opus validation. Branch `fix/macos-notification-permission` in the main checkout.

## Progress / evidence
- 2026-10-24: doc created, branch created from main e483dfe.
- 2026-10-24 (writer): N1 done. `adapters/notify/macos.rs` (objc2-user-notifications 0.3.2, block2 0.6.2, objc2 0.6.5, objc2-foundation 0.3.2; no new versions in Cargo.lock): bundle check through `NSBundle.mainBundle.bundleIdentifier` (without it `currentNotificationCenter` raises, so it is never called), `getNotificationSettings`, `requestAuthorization(alert|sound)`, delivery with a delegate so banners show in the foreground, `open` of the Notifications pane (fallback URL). `notify-rust` is now a Windows-only dependency. Pure logic in `domain/permission.rs` (`PermissionState`, mapping, when to ask) with 7 unit tests (tests written together with the code, no separate RED run).
- N2 done: Settings > Alerts shows a "System permission" row under the switch (state line, "Open System Settings" when denied or refused); the shell reads the state at startup and when Settings opens, asks only when the switch goes off to on or at startup with the switch on and status not determined; the sink never prompts twice (only a not-determined status triggers the prompt). Demo runs read the real permission but show nothing. Info.plist needed no new keys (the bundle identifier `dev.natikube.app` is enough).
- Checks: fmt OK; clippy -D warnings OK; workspace tests 580 passed, 0 failed, 3 ignored; release build OK; `scripts/bundle.sh --skip-build` OK (ad-hoc signed, plutil OK).
- Real run (bundle executable, isolated `NATIKUBE_CONFIG_DIR`, hidden `--request-notification-permission`): `permission before: NotDetermined`; the request then stayed pending (callback not called after 25+ s, no NSError, no refusal), no NatiKube entry in `defaults read com.apple.ncprefs` yet. So macOS did NOT refuse the ad-hoc bundle up front, and the status query works from the bundle. The prompt could not be answered from the session: the final result (granted, denied or refused with its NSError) is still unknown.
- Screenshot of Settings > Alerts: the Mac session was locked, `screencapture` returned "could not create image from window"; the first capture (before the permission row was below the fold) only showed the Rules section. Pending.
- Not done: no real notification was sent (0 of 2 used).
- 2026-10-24 (fix, V1): `UserNotificationSink::show` no longer calls `ask()`; with a NotDetermined status it returns `NotifyError(blocked_reason)` ("Notifications were not allowed yet."). Only `request_permission` (switch on, startup with switch on) prompts, so alert dispatch on the shared Tokio runtime can never block on the 600 s prompt wait. The optional `spawn_blocking` move was not done (status round-trip is capped at 10 s). Checks: fmt OK; clippy -D warnings OK; workspace tests 580 passed, 0 failed; release build OK; `bundle.sh --skip-build` OK. Re-run of the bundled binary with `--demo --request-notification-permission` printed no log lines to the pipe; permission prompt still unanswered, screenshot still pending (session locked). 0 of 2 real notifications used.

- 2026-10-24 (writer, N4/N5/N6): code and logic done, visual check pending (see below).
  - Core `AlertsConfig` (tolerant, defaults): `in_app_toasts` true, `sound` false, `sound_critical_only` false, `sound_name` Glass (macOS/Linux) or SystemNotification (Windows); an unknown name clamps to the default. RED first (compile errors on the missing fields), then GREEN: 4 new tests.
  - Domain (pure, RED with `todo!()` stubs: 18 failing, then GREEN 18 passing): `ToastQueue` (max 3, oldest evicted, 6 s TTL, hover keeps it and restarts the timer on leave, ids never reused) and `plan_batch`/`SoundThrottle` (once per batch, critical-only filter, 2 s window, a filtered batch does not use up the window, toasts and sound independent). `AlertDispatcher::set_on_fired` reports each stored batch with its row ids (2 tests, written with the code). `AlertsEdit` gets InAppToasts/Sound/SoundCriticalOnly/SoundName (2 tests, written with the code).
  - UI: `ui/toast` `ToastHost` (bottom right, above the status bar, below palette and modals; severity bar, title, cluster, object, message; close cross; hover; click marks read and opens the object through the same `BellEvent`). Settings > Alerts: "In-app notifications" and "Sound" sections (switch, critical-only switch, Preview, 14 or 5 sound chips; dimmed while sound is off).
  - Shell: second channel from `on_fired`, UI task merges bursts, `plan_batch` with the current config, toasts pushed and sound started; `AppCommand::PreviewAlertSound`. Nothing blocks the UI thread: sound goes to a worker thread on macOS (the first `NSSound` call took 214 ms measured on the UI-like main thread; after the change `play` returns in 248 us) and `PlaySoundW` is `SND_ASYNC` on Windows.
  - Native deps: only versions already in Cargo.lock (objc2-app-kit 0.3.2 with the NSSound feature; windows-sys 0.61.2 with Win32_Media_Audio). Cargo.lock only gained dependency edges. No audio crate.
  - N6: `PermissionState::status_text` shows the refusal's NSError verbatim under the status line (2 tests, RED then GREEN); `PermissionCache` keeps an "allowed" so `show` skips the status round-trip, forgets it on any other answer or a failed delivery (2 tests, RED then GREEN). The delivery wait itself (up to 10 s, normally milliseconds) is still on the caller's thread.
  - Real checks: `--demo --play-sound Glass` twice (2 of 3 sounds used): `play` returned in 248 us and the worker reported `Ok(())`; no OS notification sent. App launched with `--demo --fake-alerts 6 --fake-live-alerts 4` (with and without `--open-settings --settings-section alerts`), alive after 6 s, no log errors.
  - Windows: `adapters/sound/windows.rs` type-checks alone against windows-sys 0.61.2 for `x86_64-pc-windows-msvc` (scratch crate). The whole crate could not be cross-checked on this Mac (ring needs MSVC headers); Windows CI is the proof.
  - Checks: fmt OK; clippy -D warnings OK; workspace tests 611 passed, 0 failed, 3 ignored; release build OK.
  - NOT done: screenshots. The Mac session was locked (`CGSSessionScreenIsLocked` = 1) and `screencapture -l` answered "could not create image from window", so toasts in Standard and Command, the four appearances and Settings > Alerts (sound area, refusal text) were never seen. Click-to-open and hover were not exercised either. N4, N5 and N6 stay open until a visual pass.

## Next step
Visual pass for N4/N5/N6 on an unlocked session (`--demo --fake-alerts 6 --fake-live-alerts 4`, Standard and Command, then `--open-settings --settings-section alerts` scrolled to Sound). Paul answers the macOS permission prompt on the bundled app (`<bundle>/Contents/MacOS/natikube --request-notification-permission`, or turn the switch on in Settings > Alerts), then confirm the app appears in System Settings > Notifications and the test notification shows. N3 stays open until then.
