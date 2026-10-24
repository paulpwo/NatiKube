# Update check on startup

## Objective
When NatiKube starts, check GitHub Releases for a newer version and, if there is one, show an optional, dismissible modal. Never block startup, never auto-download, never nag.

## Why
Paul (2026-10-24): "necesito que al abrir el programa verifique si hay nueva version y lo indique como opcional en un modal dimisible". NatiKube ships unsigned builds from GitHub Releases (alpha, pre-releases), so users have no store to tell them about updates.

## Scope
- Source: `GET https://api.github.com/repos/paulpwo/NatiKube/releases?per_page=10` (not `/releases/latest`, which skips pre-releases and every release is a pre-release during the alpha). Pick the highest semver among non-draft releases whose tag parses as `v<semver>`. Compare with `CARGO_PKG_VERSION`.
- Runs once per launch, off the UI thread, with a short timeout (about 5 s). Any failure (offline, rate limit, bad JSON) is silent: log at debug level, show nothing.
- Modal: "NatiKube <new> is available" / "You have <current>." plus the release name and a short excerpt of the notes, buttons **Download** (opens the release page in the browser), **Later** (closes; asks again next launch), **Skip this version** (stores the version; asks again only for a newer one). Escape and clicking outside close it like Later. Styled with the active appearance tokens.
- Settings: `updates.check_on_startup` (default true) and `updates.skipped_version` (optional) in `settings.json`, tolerant like the rest of the file; a toggle in the Settings window.
- Never runs with `--demo`, `--exit-after-first-frame`, `--dump*`, or when `NATIKUBE_NO_UPDATE_CHECK=1`.
- Only `api.github.com` and the release page URL are contacted. No telemetry, no identifiers beyond a `User-Agent: NatiKube/<version>` header that GitHub requires.

## Constraints
- No new HTTP stack: reuse the hyper / hyper-util / hyper-rustls / rustls versions already in Cargo.lock (or a tiny equivalent); no reqwest, no web runtime crates.
- Domain logic (version selection, skip rule, "should check" rule) is pure and unit tested; the HTTP adapter is thin.
- Works on macOS and Windows.

## Tasks
- [x] U1 Domain: release model, newest-release selection over pre-releases, comparison with the current version, skip rule, "should check" rule (flags, env, setting). Tests first.
- [x] U2 Adapter: GitHub releases fetch with timeout and User-Agent, JSON parsing into the domain model; parsing tested on a fixture.
- [x] U3 Settings: `updates` prefs in core settings (tolerant load, defaults), Settings window toggle.
- [x] U4 UI: dismissible modal with Download / Later / Skip this version, wired at startup in the shell; Escape and outside click close it.
- [x] U5 Docs: README (privacy note: one request to api.github.com on startup, how to turn it off), architecture.md.

## Acceptance
- With a mocked newer release, the modal shows; Later closes; Skip stores the version and suppresses it; Download opens the release URL.
- Offline or API failure: no modal, no error dialog, startup time unaffected.
- fmt, clippy -D warnings, workspace tests green on macOS; CI green on Windows.

## Route
Delegated direct: one Sonnet writer (new feature slice + core settings + shell wiring are 3+ non-trivial files), then Opus validation.

## Progress / evidence
- 2026-10-24: doc created.
- 2026-10-24 writer (delegated direct, one Sonnet writer; trigger: new slice + core + settings + shell + app = 3+ non-trivial files). No commits made (parent owns git).
  - U1: `crates/features/updates/src/domain` (release.rs, policy.rs). 9 unit tests green (newest over pre-releases and `alpha.10` vs `alpha.9`, final beats rc, strict-newer compare, skip rule, excerpt, should_check incl. env/demo/exit-after-first-frame). RED was not observed as a separate run: tests and implementation were written together in the same files.
  - U2: `adapters/github.rs`, hyper + hyper-rustls (ring, native roots) + 5 s timeout, no redirects, 1 MiB cap, `User-Agent: NatiKube/<version>`, `Accept: application/vnd.github+json`. `tests/github_fixture.rs` (5 tests) over `tests/fixtures/releases.json` (draft, `nightly`, wrong-shape entry, foreign html_url). Cargo.lock gained only the `natikube-updates` entry, no third-party crate.
  - U3: `UpdatePrefs { check_on_startup = true, skipped_version = None }` in core (tolerant, trimmed), 3 new core tests; Settings window gets an Updates section with the toggle (`PrefEdit::CheckUpdatesOnStartup`), 1 new form test, sections test updated.
  - U4: `UpdateModal` (ui) + shell `update_ops.rs`. The check starts from the first render, fetch runs on `natikube_kube_client::spawn`, result delivered to a GPUI task; failures only `tracing::debug!`. Download uses `natikube_portforward::domain::open_url` (no helper moved). Hidden `--fake-update <version>` overrides `--demo` and the setting; its Skip is not persisted.
  - U5: README "Updates and privacy" section, docs/architecture.md (crate row, updates contract, settings, flags).
  - Checks: cargo fmt --check OK; clippy --workspace --all-targets -D warnings OK; cargo test --workspace all green; cargo build --release OK.
  - Visual: release binary `--demo --fake-update 9.9.9` for standard and command appearances, window-only screenshots in the session scratchpad (`update-modal-standard.png`, `update-modal-command.png`).
  - Not verified: real GitHub request (no network call was made; the repo has no published release newer than 0.1.0 to test against), Escape / outside click / button clicks by hand (no input automation), Windows.

## Next step
Opus validation, then Paul reviews before any commit.
