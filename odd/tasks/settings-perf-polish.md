# Settings window + performance + polish

## Objective
Make NatiKube as fast and light as possible, as beautiful as possible in all four appearances, and add a Settings window. No numeric promises (Paul, 2026-10-23): measure, optimize, report before/after.

## Why
Paul: "quiero que esto vaya a velocidad de la luz y sea súper ligero, que casi no consuma memoria RAM", "no prometas nada, solo optimízame y que sea lo más lindo posible", and "debo tener un menú de settings: cambiar el template, tamaño de la fuente, cosas así".

## Baseline (from appearances workflow, 2026-10-23)
Release binary 19.3 MB; first frame 254–298 ms warm; idle RSS after 10 s 122.4 MB (dev-cluster) / 127.6 MB (10k fake rows).

## Tasks
### Carry-over fixes (Opus recheck of appearances workflow)
- [x] C1 (V1) Alias fold must rank by source first (~/.kube/config, $KUBECONFIG order, ~/.kube others, extras, newest mtime) and only then by shortest name; the primary source must not be an old backup when the same cluster is in ~/.kube/config.
- [x] C2 (V2) Startup cluster lookup (`--context`, kubeconfig current-context, saved last cluster) must match aliases too.
- [x] C3 (V3) Native at the default window width must show every column (no Status cut off).
- [x] C4 (V4) Symlink + target in ~/.kube: canonicalize and prefer the real file name.
- [x] C5 (V5) Bring appearances-and-kubeconfig-discovery.md up to date.

### Performance
- [x] O1 Measure first: RSS breakdown (footprint, vmmap --summary), allocations (xctrace Allocations or dhat), binary composition (cargo-bloat), startup timeline. Record a baseline table.
- [x] O2 Memory held by resource data: store compact rows/fields instead of full objects; strip managedFields and last-applied annotations at ingestion; share strings; bound caches; drop watcher stores of views not visible.
- [x] O3 Startup: move kubeconfig scan, settings load, metrics and discovery off the critical path; first frame never waits for I/O.
- [x] O4 Rendering: virtualized lists everywhere, no per-frame allocations in hot render paths, coalesced redraws verified, idle CPU ~0 when nothing changes.
- [x] O5 Allocator and build profile: evaluate mimalloc vs system allocator, lto/codegen-units/opt-level/panic strategy; keep only changes that measurably help.

### Polish
- [x] L1 All four appearances reviewed against the canvas (the design canvas) in light and dark (Native): spacing, alignment, typography scale, truncation (e.g. "Warı" header), detail panel never hides table columns (overlay vs split behaviour), empty states, hover/selection states.
- [x] L2 Fonts: evaluate embedding OFL fonts from the canvas (Hanken Grotesk for Command, IBM Plex Sans + JetBrains Mono for Observatory); keep only if size/RAM cost is small, otherwise refine with system fonts. Record the decision.

### Settings window
- [x] S1 New crate `crates/features/settings`: native-style Settings window opened with ⌘, and App menu → Settings….
- [x] S2 Sections: Appearance (template Standard/Command/Native/Observatory, UI font size/zoom, density compact/comfortable, monospace font + size), Clusters (kubeconfig sources list with add/remove, Import from Freelens, rescan), Terminal (font size, shell), Logs (tail lines, timestamps, wrap default), Data (metrics poll interval, default namespace).
- [x] S3 Everything persisted in settings.json (backward compatible, unknown values tolerated) and applied live.

## Constraints
- Agents: read-only on `dev-cluster` only; never connect to EKS. No commits. No web runtime. Standard appearance stays recognizably the same (polish allowed, no redesign).

## Acceptance
- fmt, clippy -D warnings, tests, release build green.
- Before/after table for size, first frame, idle RSS (dev-cluster and 10k fake rows), idle CPU.
- Screenshots: each appearance (Native light and dark) and the Settings window.

## Route
Workflow: Measure (Sonnet, read-only) → Foundation (Sonnet: settings crate skeleton, Settings fields, contracts) → 3 parallel Sonnet writers (settings crate S1–S3; perf in resources + kube-client O2–O4; clusters C1/C2/C4) → Integrate + polish (Sonnet: shell/ui-kit/detail L1–L2, C3, O5, menus, measurements, screenshots, docs C5) → Opus validate → Sonnet fix → Opus recheck.

## Progress / evidence
- 2026-10-23: doc created. Route: workflow. Measure (read-only profiler) -> Foundation -> 3 parallel writers (settings S1-S3, perf O2/O3/O4 in resources + kube-client, clusters C1/C2/C4) -> this integration + polish writer (alone, inline) -> Opus validate -> fix -> recheck (the last three are still pending).
- Route per task (integration writer): everything inline in one writer; no sub-delegation. Trigger evidence: edits touched ~25 files but each was mechanical wiring against code the writer had just read.

### Carry-over fixes
- C1, C4 (clusters writer): rank by source first, then name length; symlink and target fold to the real file. Tests in crates/features/clusters/tests/discovery.rs (RED = compile error E0432 for the missing functions, then green). Real run: `--dump-clusters` now lists the ARN-named `prod-cluster` from `~/.kube/config` as primary (its backups are folded) and `dev-cluster`.
- C2: `find_cluster` (id, context, name, aliases) is used by `crates/app/src/main.rs` for `--context` and current-context, and by `ShellView::new` for the saved last cluster (id, then bare context). Observed: `--context dev-cluster` resolves to the existing entry without adding one; `--context prod-cluster` is not in any file any more, so it is injected as a bare context (expected). Alias matching itself is covered by unit tests only, because the real kubeconfigs no longer contain two names for one cluster.
- C3: Native shows every column at the default 1360 px window (screenshot natikube-v2-native-light.png / -native-dark.png: Name .. Status plus the row menu, no horizontal scroll). Mechanism: `fit_widths` (crates/features/resources/src/domain/object.rs, 4 tests in tests/columns.rs, RED = unresolved import) shrinks the non-name columns first (to 75 %), the name column only after that (to 60 %), marker columns never. The table measures its own width in a canvas and re-fits when it changes (also when the inspector opens, 360 px). Applied to Command, Native and Observatory; Standard keeps its designed widths and scrolls as before. The 36 px Warning column header is now empty instead of the clipped "War".
- C5: odd/tasks/appearances-and-kubeconfig-discovery.md brought up to date (see its "Update 2026-10-23 (settings-perf-polish)" section).

### Settings (S1-S3)
- S1/S2 (settings writer): `crates/features/settings`, window 760x560, five sections (screenshot natikube-v2-settings.png). Integration: the Template row now stacks its control under the label (the description wrapped into a 4-character column before).
- S3 live application, all wired in `crates/shell/src/settings_ops.rs` and ui-kit:
  - Appearance, font size, zoom, density, monospace family and size: `natikube_ui_kit::set_ui_prefs` stores the prefs and re-applies the active appearance. `Tokens::for_appearance_with(.., &UiPrefs)` (tests in crates/ui-kit/tests/tokens.rs, 4 new) scales font, rows, headers and column widths by `scale = font_size/13 * zoom`, compact density shrinks rows/headers (x0.86 / x0.9), and the GPUI rem size follows `scale` so spacing and text scale together. Sidebar/inspector widths scale too. Default prefs reproduce the designed tokens exactly (test).
  - Verified in the running app: a settings.json with font 16, zoom 110, compact rendered the Command appearance with larger text and compact rows (scratchpad l1/command-zoom.png). Not exercised: clicking the steppers in the Settings window (no input automation here).
  - Terminal font size/monospace family: `natikube_terminal::set_font` (live, read on every paint); shell: `terminal.shell` used by `OpenLocalShell` (`login_shell_spec_for`, test added).
  - Logs: tail lines, timestamps, wrap defaults applied when a log tab opens (`LogsView::with_prefs`); the log font follows the monospace tokens.
  - Data: metrics poll interval restarts the poller live (`ResourcesView::set_poll_interval`); default namespace via `Settings::initial_namespaces` (test): applies to a cluster whose saved filter is All/empty; a saved explicit filter wins.
  - Cmd+, and App menu -> Settings... open or focus the window (existing wiring); new run-only flag `--open-settings` for screenshots.
  - Only the main window closing quits the app.

### Performance
- Baseline (profiler, same Mac, release): see the table below. "Before" numbers are the profiler's.
- O2/O4 (perf writer): compact rows, interned strings, CRD discovery parsed through minimal structs, age tick that sleeps as long as the visible text allows, notifications only on change. O3 (perf writer + this writer): `connect_unverified` (no `GET /version` probe), visible list first, feeds and metrics after the first list; shell now uses `connect_unverified` and derives the rail dot from the list (`ResourcesView::load_state`: loading -> connecting, first list -> connected, "Cannot reach the cluster" -> failed). Catalog reload and Freelens import use `load_catalog_async` / `import_freelens_async` (short-lived worker thread). Startup still reads settings and the catalog synchronously before the window (about 20 ms, not worth the restructure; overlap with window creation was NOT done).
- O5 (measured, none adopted): see "O5 experiments".

| metric | before (profiler) | after (this build) |
|---|---|---|
| binary size | 20,258,896 B | 20,573,280 B (+314 KB: Settings crate, Hanken Grotesk 133 KB, new code) |
| first frame, 10k fake rows | 436, 321, 295 ms | 305, 265, 270 ms; alternating runs vs the pre-scale build 252-294 vs 256-269 (machine noise dominates, +-40 ms) |
| first rows frame, 10k fake | 514, 395, 376 ms | 382, 349, 352 ms |
| first frame, dev-cluster | 302, 281, 264 ms | 291, 279, 265 ms |
| first rows frame, dev-cluster | 2229, 6724, timeout (reruns: timeouts, 2319) | 1359, 1230, 2908 ms (earlier build: 1475, 1529, 1619); kubectl 0.73 s |
| idle RSS (ps) 10 s, 10k fake | 131.2 MB | 125.2 MB |
| idle RSS (ps) 10 s, dev-cluster | 110.2 MB (rows often never loaded) | 130.4 MB (rows loaded; earlier build 120.9 MB) |
| phys footprint, 10k fake | 103 MB | 90 MB |
| phys footprint, dev-cluster | 101 MB | 86-103 MB (two runs; IOSurface 36-55 MB depending on swapchain surfaces) |
| idle CPU, 10k fake (3x3 s) | 0.0, 1.9, 1.8 % | 0.0, 0.1, 1.5 % |
| idle CPU, dev-cluster | 0.0, 1.4, 1.4 % | 0.0, 0.6, 0.4 % |

Honest reading: the clear wins are time to first rows on a real cluster (the baseline had 2-15 s and timeouts, now 1.2-2.9 s in 6 of 6 runs), idle CPU, and phys footprint with 10k rows (-13 MB). RSS on dev-cluster went UP versus the profiler baseline, but that baseline mostly never loaded rows, so it is not like-for-like; RSS and footprint also fluctuate +-10 MB between identical runs. First-frame differences are inside noise. Footprint is dominated by IOSurface (3 drawables x window pixels; gpui hard-codes `set_maximum_drawable_count(3)` in gpui-pre-apple metal_renderer.rs:183, no knob): a smaller default window is the only lever (not changed; it is Paul's call).

### O5 experiments (release builds in scratch target dirs; main target untouched)
- mimalloc (`--features mimalloc`): +218 KB binary, RSS 129.5 vs 126.0 MB, footprint 92 vs 91 MB, no first-frame change. Rejected; the `#[global_allocator]` / dhat hooks stay behind the off-by-default features.
- `opt-level="s"` for every dependency: binary 17.6 MB (-2.9 MB, -14 %), first frame 224-252 ms vs 198-200 ms and first rows 285-315 vs 256-261 ms in the same sitting, footprint 90 MB (no change). Rejected: it trades speed for disk size only.
- `opt-level="s"` only for k8s-openapi, kube*, rustls, image decoders, usvg/resvg, regex, serde_saphyr: -0.64 MB (-3 %), no speed change, no footprint change (one noisy 116 MB sample). Rejected as not worth a profile entry.
- Not tried: trimming gpui/gpui-component features (image decoders), tokio runtime size (kept at 2 workers).

### Polish
- L1: reviewed Standard, Command, Native light, Native dark, Observatory and Settings against the canvas by screenshot (scratchpad natikube-v2-*.png). Fixed: Native columns (C3), clipped Warning header, Settings Template row, column and sidebar widths now follow font size/zoom (headers no longer clip at 16 px x 110 %). Not pixel-compared with the canvases. Known leftovers: Standard still scrolls horizontally at 1360 px (unchanged by design); when the inspector is open on Native the table may still scroll slightly because the name column floor is 60 %; Native light under a dark macOS shows a dark title bar when forced with `--native-mode light` (system title bar, only seen in forced mode).
- New run-only flag `--native-mode light|dark` (not persisted) so Native light and dark can be screenshotted regardless of macOS.
- L2 fonts: Hanken Grotesk is embedded for Command only (variable TTF, 132,892 B = 0.65 % of the binary, SIL OFL; crates/ui-kit/assets/HankenGrotesk-Variable.ttf, registered by `register_fonts`; Command falls back to the system UI font if registration fails). Rendering verified in the running app. Not embedded: IBM Plex Sans (Regular alone is 187 KB, Observatory needs several weights, about 0.75 MB or +3.7 %) and JetBrains Mono (not fetched); Observatory and the other appearances keep the system fonts (SF Pro, Menlo). Footprint with the font loaded: 86-90 MB on dev-cluster/fake, i.e. no visible RAM cost.

## Checks (final)
- cargo fmt --all -- --check: clean.
- cargo clippy --workspace --all-targets -- -D warnings: clean.
- cargo test --workspace: 397 passed, 0 failed, 1 ignored.
- cargo build --release: OK.

## Gaps
- Settings window controls were not clicked (no input automation); live application was verified through settings.json at startup and by unit tests of the token math. Text inputs in the Settings window do not resync from external changes. No Escape/Cmd+W close binding in the Settings window.
- Alias-aware startup lookup is covered by unit tests, not by a real kubeconfig with two names for one cluster.
- Startup still loads settings and the catalog before the window (about 20 ms).
- Idle CPU is not exactly 0 (0.1-1.5 % in the last samples; the main thread is blocked in the run loop and the remainder comes from the GPUI display link and the metrics poll).
- RED/GREEN: ui-kit token tests and the terminal/logs wiring were written with or after the code (no separate RED); RED observed for `initial_namespaces`, `fit_widths`.
- No commits were made; settings.json was restored from the backup after every run.

## Next step
Opus validation of this integration, then Paul reviews the screenshots against the canvases, decides on the default window size (GPU memory), and decides on commits.

## Fix round: F1
- F1 (major, Standard default size hid Status and clipped Age): fixed in crates/features/resources/src/ui/objects_table.rs by applying fit_widths to every appearance (removed the Standard exclusions in apply_look and fit_to). Designed widths remain the upper bound. fmt, clippy -D warnings, workspace tests and release build pass. Screenshot not retaken (no visual run performed; settings.json untouched).

## Carry-over findings after the Opus validation (release-pipeline-and-windows workflow)
- V1 (settings.json syntax error silently replaced by defaults): fixed. `FileSettingsStore` keeps the broken file as `.bak`, refuses to overwrite it and `load_reporting()` returns a warning; `natikube-app` now uses it and the shell shows the warning in the status bar.
- F2 (rail said Connected without reaching the cluster): fixed for overviews. `ResourcesView::load_state` now reports Loading for a real connection until the namespace feed gets an answer (`overview_state`, unit tested); lists already derived it from their first batch. An API server that is unreachable while an overview is open still shows Connecting, not Failed.
- F4 (one bad per-cluster entry dropped all): fixed, per-entry tolerant parsing (tests in crates/core/tests/settings.rs).
- F6 (text inputs persisted per keystroke): fixed. 400 ms debounce, flush on Blur/Enter and on window close (`on_window_should_close`), and `BackgroundSettingsStore` writes off the UI thread, flushed on quit (`on_app_quit` and after `run`).
- V3/F7 (header clipping): Command and Native pod column sets and header-aware widths fixed by the demo stream; Standard at 1360 px verified by screenshot only for readability, not pixel-compared.
- V4 (Command sidebar showed the raw ARN): fixed in `ClusterInfo::new` via `short_context_name` (EKS ARN shows `<name>`; context and id unchanged). Test in crates/core/tests/cluster.rs.
- F8 (stale doc comment): the finding was not specific enough to locate; only the `--native-mode` help text ("follows macOS") was corrected. The finding could not be matched to a specific comment, so it stays open.
