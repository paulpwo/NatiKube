# Release pipeline (GitHub Actions) + Windows build + unsigned install docs

## Objective
1. Publish NatiKube on GitHub Releases without an Apple Developer ID (Paul will not pay the 99 USD): ad-hoc signed macOS app in a DMG, SHA-256 checksums, and clear install instructions (quarantine removal).
2. A GitHub Actions release workflow that builds and attaches the artifacts on every `v*` tag, plus a CI workflow for pushes and PRs.
3. A Windows build and release artifact.

## Why
Paul (2026-10-23): "pon las instrucciones claras, porque no pienso pagar los $99", "ve preparando la GitHub Action para que compilen la release", "si podemos tener también la release para Windows, sería genial". License: MIT (`LICENSE`).

## Tasks
### macOS packaging
- [x] M1 `scripts/bundle.sh` uses `assets/icon/NatiKube.icns`, sets version from the workspace package, and ad-hoc signs the bundle (`codesign --force --deep --options runtime --sign -`).
- [x] M2 `scripts/package-macos.sh`: bundle → DMG (hdiutil, with an Applications symlink) → `NatiKube-<version>-macos-arm64.dmg` + `.sha256`.
- [x] M3 Runtime Dock icon when running the bare binary (set the application icon image from the embedded PNG).

### Windows
- [ ] W1 Port platform assumptions behind `cfg(target_os)` (files found by grep: core/src/prefs.rs, clusters adapters settings_file.rs, environment.rs, freelens.rs, picker.rs, domain/login_path.rs, terminal adapters local_pty.rs, pod_exec.rs, mod.rs, ui/terminal_view.rs, portforward ui/forwards_view.rs, settings ui/settings_view.rs and their tests): settings dir via `dirs::config_dir()` (`%APPDATA%\NatiKube`), kubeconfig `%USERPROFILE%\.kube`, Freelens store `%APPDATA%\Freelens`, default shell `pwsh.exe` → `powershell.exe` → `cmd.exe` via ConPTY, open URL with `cmd /C start`, no login-shell PATH step, menus degrade gracefully, window icon from `.ico`.
- [ ] W2 `assets/icon/natikube.ico` (16–256 px) and Windows resource embedding (exe icon, version info) via a build script (`winresource` or `embed-resource`, Windows-only build dependency).
- [ ] W3 `scripts/package-windows.ps1`: release build → `NatiKube-<version>-windows-x64.zip` (exe + LICENSE + third-party notices) + `.sha256`. MSI is optional later.

### GitHub Actions
- [ ] G1 `.github/workflows/ci.yml`: on push/PR → fmt check, clippy -D warnings, tests on `macos-latest` and `windows-latest`, cargo cache.
- [ ] G2 `.github/workflows/release.yml`: on tag `v*` → build macOS arm64 (and optionally x86_64 + universal via lipo) and Windows x64, package, create the GitHub Release with artifacts, checksums and release notes that include the install instructions.

### Docs
- [x] D1 README: install from GitHub Releases on macOS (drag to Applications; `xattr -dr com.apple.quarantine /Applications/NatiKube.app`; or System Settings → Privacy & Security → Open Anyway on macOS 15+; verify with `shasum -a 256`), Windows (unzip; SmartScreen "More info → Run anyway"), Homebrew cask note, build from source.
- [ ] D2 Workspace `license = "MIT"` in Cargo.toml; THIRD-PARTY-NOTICES generated (cargo-about or a script over `cargo metadata`) and shipped in both artifacts and the About window.

### Public repo (Paul approved 2026-10-23: public repo `paulpwo/NatiKube`, exact app name)
- [x] R1 `--demo` mode: a curated, generic demo cluster (namespaces like `shop`, `payments`, `auth`, `monitoring`, `ingress`; realistic pod names, CPU/memory, mixed statuses incl. CrashLoopBackOff and Pending, a few deployments/services/nodes/events) used for README screenshots. No real cluster data.
- [x] R2 README "bien bonito": light/dark wordmark header via `<picture>`, badges (license MIT, platform macOS/Windows, built with Rust/GPUI), a hero screenshot plus a gallery of the four appearances and the Settings window (`docs/screenshots/*.png`, taken with `--demo`, window only), features list, install (D1), build from source, keyboard shortcuts, architecture link, acknowledgements, license.
- [x] R3 Scrub before the first commit: no company or personal infrastructure data in the repo (real pod/namespace names from Paul's clusters, node names, internal IPs, AWS account IDs/ARNs, kubeconfig contents, tokens, absolute personal paths where avoidable). Known hits: `docs/design/*.dc.html` (real pod names, a node name). Replace with the demo data set.
- [ ] R4 Create the public GitHub repo `paulpwo/NatiKube` with description and topics, first Conventional Commit authored by Paul's git identity with no AI attribution, push `main`, watch CI (macOS + Windows) and fix until green.

## Constraints
- No Apple Developer ID, no notarization, no paid certificates (also none for Windows code signing).
- No commits, pushes or GitHub repo creation without Paul's approval. Workflows are written and validated locally (actionlint if available); they run once the repo exists on GitHub.
- Windows runtime cannot be exercised on this Mac: compile/test verification happens in CI (or local cross-check if Paul approves installing cargo-xwin + the MSVC SDK).

## Acceptance
- Local: `scripts/package-macos.sh` produces a DMG that opens on this Mac after `xattr` removal; fmt/clippy/tests green.
- CI files pass actionlint; Windows build green on GitHub Actions once pushed.

## Route
After the running settings-perf-polish workflow: one Sonnet writer for macOS packaging + docs + Actions, one Sonnet writer for the Windows port (disjoint files), Opus validation.

## Progress / evidence
- 2026-10-23: doc created; GPUI Windows support confirmed (`gpui-pre-platform` depends on `gpui-pre-windows` for `target_os = "windows"`); 14 files with macOS assumptions identified.

- 2026-10-23 (integration writer, alone): 
  - M1/M2 observed: `scripts/package-macos.sh` produced `target/dist/NatiKube-0.1.0-macos-arm64.dmg` (11,368,106 bytes, sha256 `cb928065770633a880b3023d6d2aa4b79d3a03b68eeed41a623163cb0d3f7b79`, `shasum -c` OK). The DMG mounts with `NatiKube.app` + `Applications`; the copied app is ad-hoc signed (`Signature=adhoc`, flags `adhoc,runtime`) and, after `xattr -dr com.apple.quarantine`, runs `--demo --exit-after-first-frame` (first_frame_ms=1055, first_rows_frame_ms=1189, exit 0).
  - M3: `crates/shell/src/dock_icon.rs` sets the Dock icon from the embedded `natikube-256.png` through objc2 (`NSApplication.setApplicationIconImage`), macOS only. Compiles and the app starts; the Dock icon itself was not looked at.
  - R1: `--demo` wired (crates/app/src/demo.rs, main.rs, shell). Catalog `demo-cluster` and `staging`, both Connected in the rail, `ResourcesView::demo` for every view, settings kept in memory (the real settings.json and kubeconfigs are never read; reload/import/add are no-ops). No network. `settings.json` was byte-identical after the runs.
  - R2: README rewritten; screenshots in `docs/screenshots/` (standard, command, native-light, native-dark, observatory at 2720x1786, settings 1520x1186), window only, from the release binary with `--demo`. All six were opened and inspected: only demo data is visible.
  - R3: grep over the whole repo (excluding target/.git) for the privacy list. Replaced the real pod and node names in `docs/design/*.dc.html` with demo values, renamed the dev context in fixtures/docs/scripts to `dev-cluster`, replaced absolute paths with `<repo>`, removed a private canvas link and the company name from a test guard list. Remaining hits are listed in the final report.
  - D2 (part): workspace license is MIT, authors and repository set; THIRD-PARTY-NOTICES live at `packaging/THIRD-PARTY-NOTICES.md` and ship in the bundle and the ZIP script. Not done: showing them in the About window.
  - W2 (part): `crates/app/build.rs` embeds `assets/icon/natikube.ico` and version info with `embed-resource` (Windows host only, build-dependency `cfg(windows)`). Not compiled on Windows here.
  - Keyboard: shell shortcuts now use `secondary-` (Cmd on macOS, Ctrl elsewhere).

## Open items (not observed)
- W1 (code done by the Windows stream), W2 embedding, W3 `package-windows.ps1`, G1 and G2 have never run on Windows or GitHub: only parsed or compiled for macOS (`cargo check --target x86_64-pc-windows-msvc` passes for natikube-core only; ring needs a C cross compiler for the rest). CI is the verification.
- actionlint is not installed, so the workflows are only YAML-parsed.
- release.yml does not check that the tag matches the package version.
- No screenshot with the detail panel open: `--demo` has no connection, so the detail panel is not created.

## Next step
R4: the orchestrator creates the public repo `paulpwo/NatiKube`, makes the first Conventional Commit with Paul's identity, pushes `main` and watches CI (macOS + Windows) until green. Then tag `v0.1.0` to exercise release.yml.

## Fix evidence: validator findings (F-KEYS-WIN, F-WIN-CONSOLE)

- F-KEYS-WIN fixed: `crates/shell/src/app.rs` global `secondary-` bindings use context `!Terminal` off macOS (`None` on macOS), via `app_key_context_for`. Tests `key_binding_tests::*` (predicate does not match a Terminal context, matches others; macOS stays global). Windows runtime not exercised here; CI verifies.
- F-WIN-CONSOLE fixed: `crates/app/src/main.rs` now starts with `#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]`. AttachConsole for `--help`/`--dump*` not added. Cannot be run here; CI verifies.
- Checks: fmt, clippy -D warnings, workspace tests, release build all pass on macOS.

## Fix evidence: Opus recheck findings

- PRIV-1 fixed: `docs/design/{Command,MacNative,Observatory}.dc.html` rows now use only names from `crates/features/resources/src/domain/demo.rs` (coredns, metrics-server, node-exporter, kube-proxy, prometheus-0, alertmanager-0, ledger-db-0, auth-redis-0); owners and namespaces adjusted to match; the second rail cluster is now `staging-demo`. Intersection check: read-only `get pods,deploy,sts,ds,svc,cm,ns,nodes -A -o name` on the dev cluster written outside the repo, then full names, hashed names and non-generic base names grepped over every publishable file (excluding Cargo.lock and THIRD-PARTY-NOTICES.md): 0 hits. Generic component words (coredns, metrics-server, kube-proxy, traefik, cert-manager, redis, monitoring...) are not forbidden bare. New `scripts/privacy-check.sh` (deny-list file via `NATIKUBE_PRIVACY_DENYLIST`, kept untracked, plus built-in ARN / ECR host / "account id" + 12 digits patterns; all-zero placeholder ids allowed) with a CI job `privacy` for the built-in patterns. Negative test: a fake ARN with a 12-digit id made it exit 1.
- WIN-EXEC-CONSOLE fixed: `kube-client-4.2.0/src/client/auth/mod.rs:625` reads `KUBE_RS_UNSTABLE_CREATE_NO_WINDOW` and enables only when the value is exactly `1`. `init_environment` (cfg windows) sets it before any thread exists unless the user already set a value; pure `no_window_value` with 3 unit tests (run on macOS); note in docs/architecture.md. Windows runtime not exercised.
- OFL-NOTICE fixed: `scripts/third-party-notices.sh` adds a "Fonts" section (Hanken Grotesk copyright line + full OFL text); `packaging/THIRD-PARTY-NOTICES.md` regenerated (538 crates). `bundle.sh` copies it into `Contents/Resources`; `package-windows.ps1` copies it into the ZIP.
- GHA-PIN-PERMS fixed: release.yml top-level `contents: read`, `contents: write` only on the `release` job. All actions in release.yml and ci.yml pinned to SHAs resolved read-only with `gh api` (annotated tag dereferenced for rust-cache), tag as comment; `dtolnay/rust-toolchain` is pinned to the current `stable` branch head with an explicit `toolchain: stable`. Workflows only YAML-checked, not run.
- BREW-STALE fixed: cask url is `https://github.com/paulpwo/NatiKube/releases/download/v#{version}/NatiKube-#{version}-macos-arm64.dmg`, sha256 is a marked placeholder, zap path `Application Support/NatiKube`; README still says "coming soon".
- WIN-CLI-SILENT fixed (documented): README Build section says CLI flags print nothing on Windows release builds; use a debug build.
- KBD fixed: search-box hint is `palette_hint(is_macos)` = `⌘K` on macOS, `Ctrl+K` elsewhere; test `palette_hint_tests`.
- Checks: fmt clean, clippy -D warnings clean, workspace tests 447 passed 0 failed, release build OK, privacy check 0 hits.
