# MVP — NatiKube (F1–F7)

## Objective
Turn the F0 spike into a usable native replacement for the daily Freelens workflow: clusters, full resource browser, detail + YAML, logs, terminal/exec, port-forward and an installable signed app. 100% native Rust + GPUI Kit, no web runtime.

## Problem / why
Paul reviewed F0 (pods table only) and found it too basic; he asked to build the whole MVP ("dale con todo"). Freelens is 576 MB Electron; F0 proved 13.4 MB / ~265 ms first frame / ~107 MB idle RSS is achievable.

## Scope (authorized 2026-10-23)
F1–F7 as below. Out of scope: Helm, Prometheus charts, detailed RBAC views, extensions, Windows/Linux, auto-update.

## Constraints
- Same stack and hexagonal layout as F0 (`core` / `k8s` / `ui` / `app`, plus `term` in F5). No webview crates ever.
- One watcher per visible view; cancel on navigation. Redraws batched.
- Cluster access by agents: read-only (list/watch/get/logs/discovery/metrics) on `dev-cluster`. Writes (apply/delete/scale/restart), exec and port-forward are implemented and unit-tested, but run against a real cluster only by Paul.
- No git commits, branches or pushes without Paul's explicit approval (user rule overrides ODD work-unit commits). All work stays untracked until he approves.
- Copy and UI text in English, matching Freelens labels where they exist.

## Tasks
### F1 — Clusters
- [x] F1.1 Kubeconfig catalog: `~/.kube/config`, `$KUBECONFIG`, extra files/folders from a settings file; list every context.
- [x] F1.2 Left cluster rail (hotbar-like) with initials avatar, active cluster, connection state (connecting / connected / error with message).
- [x] F1.3 Exec credential plugins work when launched from Finder: resolve PATH from the user's login shell at startup.
- [x] F1.4 Persist last cluster, namespace and view in a local settings file.

### F2 — Resource browser
- [x] F2.1 Sidebar like Freelens: Cluster overview, Nodes, Workloads (Overview, Pods, Deployments, DaemonSets, StatefulSets, ReplicaSets, ReplicationControllers, Jobs, CronJobs), Config (ConfigMaps, Secrets, ResourceQuotas, LimitRanges, HPA, PDB, PriorityClasses, RuntimeClasses, Leases, Webhooks), Network (Services, Endpoints, EndpointSlices, Ingresses, IngressClasses, NetworkPolicies), Storage (PVC, PV, StorageClasses), Namespaces, Events, Access Control (ServiceAccounts, Roles, RoleBindings, ClusterRoles, ClusterRoleBindings — list only), Custom Resources (CRDs grouped by API group, generic tables).
- [x] F2.2 Generic table engine over discovery + dynamic objects, with kind-specific columns for built-in kinds (kubectl-like) and Name/Namespace/Age fallback for CRDs.
- [x] F2.3 Namespace filter (all / multi-select), search box, sortable columns, item count, top tabs for the Workloads group.
- [x] F2.4 Pod CPU and memory columns from `metrics.k8s.io` (polled), empty when metrics-server is absent.
- [ ] F2.5 Workloads overview and Cluster overview (counts by status, node capacity/usage when metrics exist).

### F3 — Detail and YAML
- [x] F3.1 Right detail panel per kind (metadata, labels, annotations, status, conditions, containers, events of the object, owner links).
- [ ] F3.2 YAML viewer/editor with Tree-sitter highlighting (GPUI Kit editor).
- [ ] F3.3 Apply with server-side dry-run validation first, then apply; delete with confirmation; scale and rollout restart for workloads. Secrets masked by default with reveal toggle.

### F4 — Logs
- [ ] F4.1 Bottom dock log tab: stream with follow, container selector, previous container, timestamps toggle, search, save to file.

### F5 — Terminal
- [ ] F5.1 `term` crate: alacritty_terminal grid rendered in GPUI (own renderer; do not copy Zed's GPL terminal code).
- [ ] F5.2 Local shell tab (portable-pty) and pod exec tab (kube ws attach), resize, copy/paste, Nerd-Font-free default font.

### F6 — Port-forward
- [ ] F6.1 Port-forward from pod/service detail, local listener, list of active forwards with stop and open-in-browser.

### F7 — Distribution
- [x] F7.1 `.app` bundle arm64 with icon and Info.plist, `scripts/bundle.sh`.
- [ ] F7.2 Codesign + notarize + DMG script (needs Paul's Developer ID; prepared, run by Paul), Homebrew cask template.

## Acceptance criteria
- Each phase: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green; release build arm64.
- F2: every sidebar entry lists the same objects as `kubectl --context dev-cluster get <kind> -A`.
- Budgets after the full MVP (goals): binary < 40 MB, idle RSS < 150 MB with one cluster, first frame < 0.5 s.
- Paul's visual QA per phase against Freelens.

## Delivery
New private repo with no commits yet; slicing into PRs is deferred until Paul approves the first commit. Forecast well above 400 changed lines per phase.

## Progress / evidence
- 2026-10-23: F0 done (see `f0-live-pods-spike.md`). MVP doc created.

- 2026-10-23: first F1+F2 writer was interrupted by a session restart before writing any source (verified: no .rs newer than this doc; F0 still builds). Paul asked for parallel multi-agent development with Sonnet and validation with Opus.

- 2026-10-23: Integration. Route: workflow with 5 parallel Sonnet writers + integrator (Opus validation pending).
  - Per-stream RED/GREEN: clusters RED observed (24 compile errors) then GREEN; resources RED observed (E0432) then GREEN, actions test had no RED; detail RED observed (83 + 23 errors) then GREEN, 2 tests without RED; logs/portforward and terminal wrote tests with code (no RED observed); UI has no RED.
  - Checks (observed): fmt --check clean; clippy --workspace --all-targets -D warnings clean; test --workspace all pass (clusters 19+3+4, core 10+5+2+6+7+7+5, detail 49, kube-client 5+4, logs 21, portforward 20, resources 2+2+2+8+6+4+2+9+2+15+5, shell 10, terminal 41, app 4); release build ok.
  - Integration changes: header dropdown filled via ClusterHeader::set_clusters; shell already routes every AppCommand (selection, navigation, detail, logs/exec/shell dock tabs, port-forwards, errors in status bar, restore of last cluster/view/namespaces).
  - Parity vs `kubectl --context dev-cluster get <kind> -A` (identical namespace/name sets): pods 17, deployments 12, daemonsets 2, statefulsets 3, replicasets 26, jobs 2, cronjobs 0, services 17, ingresses 4, configmaps 18, secrets 19 (names only), persistentvolumeclaims 3, persistentvolumes 3, storageclasses 1, namespaces 8, nodes 1, events 14, serviceaccounts 55, clusterroles 89, customresourcedefinitions 36. The `pvc`/`pv` short names are not accepted by `--dump` (use the plural).
  - Measurements (release): binary 19.0 MB (F0 13.4), first frame 255-330 ms (F0 ~265), first rows fake 10k ~360-400 ms, first rows live dev-cluster 2.0 s (one 9.4 s outlier, network), idle RSS 118.7 MB live / 122.7 MB fake (F0 ~107). Targets: <40 MB ok, <0.5 s first frame ok, <150 MB ok.
  - F7: scripts/bundle.sh builds target/NatiKube.app (arm64, plutil OK, icon generated) and it launched with --fake-rows --exit-after-first-frame. scripts/sign-notarize.sh and packaging/homebrew/natikube.rb written, NOT run (need Paul's credentials), so F7.2 stays open.
  - Screenshot of the live pods view observed (rail, sidebar, tabs, table with CPU, detail panel). Deployments screenshot not taken (no navigation flag; no input automation to avoid touching Paul's settings).
  - Not verified on screen: F2.5 overviews, Memory/Restarts columns with the detail panel open, F3.2/F3.3 flows (never clicked, read-only rule), F4 logs, F5 terminal, F6 port-forward. These stay unchecked.
  - Known: in the first runs of the new bundle two --fake-rows 100 runs timed out waiting for first rows; 8 later runs were all fine (~330 ms). Not reproduced.
  - Validator fixes (V1-V3): V1 Secret `last-applied-configuration` annotation now masked in the YAML view (mask_secret_data), in the Overview annotations (DetailModel::build) and counted by contains_mask; tests added. V2 Custom Resources now start with a reachable "Definitions" entry and discovered kinds render as collapsible API-group subsections in the sidebar (NavState::custom_groups/toggle_group), and the dynamic section no longer produces top tabs; RED observed (5 compile errors) then GREEN; F2.1 re-checked. V3 DetailPanel::clear() drops the object watcher and the events poller; CloseDetail calls it (no deterministic unit test: GPUI task handles; verified by code path only).

## Route
Delegated via one Workflow: Foundation (1 Sonnet: restructure into feature crates + shared contracts + all deps) → Build (5 parallel Sonnet writers, each owning one `crates/features/<name>/**`: clusters F1, resources F2, detail F3, logs+portforward F4/F6, terminal F5) → Integrate (1 Sonnet: shell wiring, workspace checks, parity, measurements, F7 scripts, this doc) → Validate (Opus) → Fix (Sonnet, only confirmed blockers) → Re-check (Opus).

## Next step
Opus validation, then fix confirmed blockers; Paul's visual QA of F2.5, F3.2-3.3, F4, F5, F6; run sign-notarize.sh with credentials.
