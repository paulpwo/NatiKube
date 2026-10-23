# Appearances + kubeconfig discovery

## Objective
1. Find every cluster the user has without manual setup: all kubeconfig files in `~/.kube/`, `$KUBECONFIG`, settings extras, plus an explicit "Import from Freelens…" and "Add kubeconfig…".
2. Let the user switch the whole UI between four appearances at runtime: **Standard** (current Freelens-like UI, unchanged), **Command** (A), **macOS Native** (B), **Observatory** (C).

## Problem / why
- Some EKS contexts only lived in backup kubeconfig files that Freelens had imported by path (`lens-cluster-store.json` `clusters[].kubeConfigPath`). NatiKube only read `~/.kube/config` / `$KUBECONFIG` (kubectl semantics), so those clusters were missing.
- Paul liked all three design directions from the canvas the design canvas and the current UI, and wants a menu to pick one (2026-10-23).

## Scope (authorized 2026-10-23)
### Kubeconfig discovery
- [x] K1 Scan top-level regular files in `~/.kube/` (skip `cache/`, `http-cache/`, non-kubeconfig files, files without contexts), plus `$KUBECONFIG` entries and settings extra files/folders (folders scanned one level).
- [x] K2 Dedupe identical contexts across files (same cluster server + user + namespace; contexts with different names, such as an EKS ARN and its short alias, fold into the shortest name with the others kept as aliases): prefer `~/.kube/config`, then `$KUBECONFIG` order, then other `~/.kube` files, then extras, then most recent mtime (source rank first, name length only after that; see the 2026-10-23 update). Keep distinct contexts with the same name but different server, disambiguated by source file. Each ClusterInfo keeps its source path (shown in the cluster tooltip/dropdown).
- [x] K3 "Import from Freelens…" (macOS menu File + button in the cluster area): read `~/Library/Application Support/Freelens/lens-cluster-store.json` (`clusters[].kubeConfigPath`, `contextName`) and the `kubeconfigs/` folder; add the paths to settings `extra_kubeconfigs`; show a summary (imported / already present / missing files). Never copy or print credentials.
- [x] K4 "Add kubeconfig…" (menu File + button): native open panel for files or folders (rfd, NSOpenPanel), appended to settings, catalog reloads live.

### Appearances
- [x] P1 Core: `Appearance { Standard, Command, Native, Observatory }` in Settings (default Standard), `AppCommand::SetAppearance`, `OpenPalette`, `ImportFreelens`, `AddKubeconfig`; CLI `--appearance <name>` overrides for one run without saving (for screenshots).
- [x] P2 ui-kit: one token set per appearance taken from the canvas files (colors, radii, fonts, row heights), exposed through one theme API; Native follows macOS light/dark (light = canvas B; dark = matching dark variant). Live switch without restart.
- [x] P3 Shell layouts: Standard = today's layout untouched. Command = top bar (cluster pill + ⌘K search + icons), sidebar with clusters list + grouped nav with counts, footer status bar with dock tabs. Native = source-list sidebar (clusters with colored squares, "Add kubeconfig…"), toolbar with title/subtitle, segmented status filter, search, right inspector as the detail panel. Observatory = labeled icon rail with cluster button, health cards row, kind chips, workloads grouped by namespace.
- [x] P4 Resources table per appearance: Command = name with namespace stacked, status pill, CPU/Memory value + mini bar; Native = zebra rows, system-blue selection, status dot; Observatory = grouped-by-namespace sections with per-group totals; Standard = unchanged.
- [x] P5 Command palette (⌘K) available in every appearance: jump to kind/resource by name, switch cluster, switch appearance, import Freelens, add kubeconfig.
- [x] P6 macOS menu bar: View → Appearance (4 items, checkmark on current), File → Import from Freelens…, Add kubeconfig…; shortcuts ⌘K palette.

## Constraints
- Same crates and dependency rules as `docs/architecture.md`; new feature crate `crates/features/palette`.
- Standard appearance must remain visually identical to today.
- Cluster access by agents: read-only on `dev-cluster` only. EKS contexts may be listed (parsed from files) but agents never connect to them.
- No commits without Paul's approval. No web runtime.

## Acceptance criteria
- `--dump-clusters` (or equivalent test) lists every context found under `~/.kube` without duplicates; unit tests cover dedupe and Freelens store parsing with fixtures.
- Switching appearance from the menu and from ⌘K re-renders the whole window live; choice persists across restarts.
- One window screenshot per appearance (`--appearance`), compared against the canvas.
- fmt, clippy -D warnings, tests, release build green; budgets still met (<40 MB, <0.5 s first frame, <150 MB RSS).

## Route
Workflow: Foundation (Sonnet: core types, ui-kit theme API contract, palette crate skeleton, settings field, CLI flag) → 4 parallel Sonnet writers (clusters K1–K4; ui-kit P2; resources P4; palette P5) → Integrate (Sonnet: shell layouts P3, menus P6, wiring, checks, screenshots, this doc) → Opus validate → Sonnet fix → Opus recheck.

## Progress / evidence
- 2026-10-23: doc created.
- 2026-10-23: implemented. Route: workflow, 5 parallel writers (clusters, ui-kit, resources, palette, shell) + integrator.
  - RED/GREEN: clusters tests/discovery.rs failed to compile (E0432), then 16 pass; resources appearance_logic.rs 10 compile errors, then 9 pass; palette ranking.rs E0432, then 7 pass; shell import/layout_nav/menus E0432, then pass. ui-kit tests written after code (no RED).
  - Checks: fmt clean; clippy --workspace --all-targets -D warnings clean; cargo test --workspace 352 passed, 0 failed, 1 ignored; release build OK.
  - Clusters (--dump-clusters, no secrets): contexts from several kubeconfig files were listed without duplicates; ARN-named twins that point at the same server and user as a short name are folded in as aliases (see fix round F1). Freelens store parsing is covered by fixture tests only; the import was not run against real files.
  - Screenshots (window only, --context dev-cluster): scratchpad natikube-standard.png, natikube-command.png, natikube-native.png, natikube-observatory.png. All four render; Standard unchanged. Not pixel-compared to the canvases.
  - Menu: the menu bar shows App, File, View, Window at runtime; the View > Appearance items could not be read (osascript lacks assistive access); covered by menus unit tests.
  - Integration fixes: resource list heading showed the id `path#context`; added ClusterId::context_name() and used it; startup now maps old bare-context last_cluster to the new ids.
  - Measurements (release, scripts/measure.sh): size 19.3 MB (<40), first frame 254-298 ms warm (first run 683 ms cold; <500 ms), idle RSS 122 MB real cluster / 128 MB fake 10k (<150), pod parity 17=17.
  - Gaps: fonts are system fonts (no embedding); palette resource list empty; Native status filter only in resource list; Observatory chip counts only for active kind; Native has a duplicated title; interactive flows (palette keys, rfd dialogs, menu clicks) not exercised.

- 2026-10-23 fix round (validator findings F1-F6), 1 writer:
  - F1 fixed: dedupe identity is now (server, user, namespace); shortest context name wins, other names go to `ClusterInfo.aliases` (tooltips, --dump-clusters). Decision taken: fold (Paul can reverse). Tests in discovery.rs (alias fold, namespace stays apart). Earlier doc claims of "different server/user" corrected.
  - F2 fixed: Observatory shell draws header, cards and chips only on non-list views; resource lists draw their own (one set).
  - F3 fixed: Native list header no longer repeats title/subtitle (toolbar owns it); inspector is not rendered until an object is selected; zebra via `render_tr` per real row (table `stripe` off, so no fake rows below the last item).
  - F4 fixed: `reload_clusters` and the Freelens import run in `background_executor().spawn`, applied with `update_in`; startup catalog stays inline.
  - F5 fixed: `ResourcesView::loaded_refs` feeds `palette_context(.., resources)`; refreshed when the palette opens. RED: layout_nav.rs compile error E0061 (4th arg), then GREEN.
  - F6 fixed: shell now uses `natikube_clusters::import_freelens` / `kubeconfig_add_summary` + `ImportSummary::apply`; duplicate `crates/shell/src/import.rs` and its test file deleted.
  - Checks: fmt ok; clippy --workspace --all-targets -D warnings clean; cargo test --workspace 351 passed, 0 failed; release build OK; --dump-clusters lists 3 entries.
  - Screenshots of the F2/F3 changes could not be retaken: screencapture -l failed ("could not create image from window"), so the existing PNGs are stale.

## Next step
Opus validation pass, then Paul reviews the screenshots against the canvases and decides on commits. Open items: palette resource jump (resources list empty), Observatory kind-chip counts, Native toolbar title duplicates the list header, runtime check of the palette Up/Down keys and the rfd dialogs, fonts (system fonts only).

## Update 2026-10-23 (settings-perf-polish, C5)
Corrections to the claims above after the Opus findings V1-V5 and the integration pass of `odd/tasks/settings-perf-polish.md`:
- Dedupe order (V1, fixed): candidates sort by (source rank, name length, newest mtime, position). The shortest-name rule from F1 now only applies inside the same rank, so `~/.kube/config` is the primary even if an old backup has a shorter alias. Symlink plus target in `~/.kube` fold by canonical path and show the real file name (V4, fixed).
- Startup lookup (V2, fixed): `--context`, the kubeconfig current-context and the saved last cluster go through `natikube_clusters::find_cluster` (id, context, name, aliases; the saved id falls back to its bare context).
- Native columns (V3, fixed): every column is visible at the default window width (column fitting, see the polish doc).
- F4 follow-up: catalog reload and the Freelens import use `load_catalog_async` / `import_freelens_async`. The first catalog load at startup is still synchronous (about 20 ms).
- Fonts: no longer "system fonts only". Command embeds Hanken Grotesk (133 KB); the other appearances keep SF Pro and Menlo (decision and cost in the polish doc, L2).
- Alias folding (an EKS ARN and its short alias) is covered by fixtures; a real kubeconfig with two names for one cluster was only seen once, and is no longer available.
- Test totals now: 397 passed, 0 failed, 1 ignored (`cargo test --workspace`); the 351/352 figures above are historical.
- Budgets re-measured (release 20.6 MB, first frame 250-300 ms warm, idle RSS 125-130 MB, footprint 86-90 MB): still inside the original budgets (<40 MB, <0.5 s, <150 MB RSS).
- Still open from the old gap list: palette resource jump needs a manual run, the rfd dialogs and menu clicks were never exercised, Observatory chip counts only for the active kind.
