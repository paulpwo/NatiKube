# F0 — Live pods spike (go/no-go)

## Objective
Prove that a 100% native Kubernetes desktop app (Rust + GPUI, no Electron, no webview) can show a live pod table from a real cluster with good size, memory, startup and scroll numbers before committing to the full MVP.

## Problem / why
Freelens 1.10.3 weighs 576 MB on macOS arm64 (Electron 262 MB, app.asar 155 MB, bundled kubectl/helm/proxy 139 MB). Seabird (Go + GTK4) is native but ships no current macOS build and looks like GNOME. NatiKube targets an arm64-native, single-binary app.

## Scope (authorized)
- Cargo workspace: `crates/core` (domain, no I/O), `crates/k8s` (kube-rs adapter), `crates/ui` (GPUI Kit views), `crates/app` (binary wiring).
- Read-only list + watch of pods in all namespaces on the `dev-cluster` kubeconfig context. No writes to any cluster.
- Pods table with columns Name, Namespace, Containers (ready/total), Restarts, Controlled By, Age, Status; namespace filter not required in F0.
- `--fake-rows <N>` flag to render N synthetic rows for scroll testing without a cluster.
- `--exit-after-first-frame` flag that logs time-to-first-frame and exits, for measurement.
- Release profile tuned for size (lto, codegen-units=1, strip).

## Constraints
- No webview: never depend on `gpui-webview` or any web runtime.
- Stack: `gpui-kit` 0.8.0 (pins `gpui-pre =0.3.9`), `kube`/`kube-runtime` 4.2.0, `k8s-openapi`, `tokio`.
- Hexagonal: `core` defines ports (`PodSource`) and the `PodRow` model; `k8s` implements them; `ui` depends only on `core`.
- Only one watcher per visible view; UI redraws are batched.
- No git commits without Paul's explicit approval.

## Tasks
- [x] T1 — Workspace + `core` domain: `PodRow`, status/restarts/age/controller derivation from pod data, unit tests first (RED → GREEN). Route: delegated (2+ non-trivial files).
- [x] T2 — `k8s` adapter: kubeconfig context selection, watcher/reflector over `Pod`, mapping `k8s_openapi::Pod` → `PodRow` with fixture tests. Route: delegated.
- [x] T3 — `ui` + `app`: GPUI Kit window with virtual table fed by a channel, `--fake-rows`, `--exit-after-first-frame`. Route: delegated.
- [x] T4 — Measurements: release binary size, cold start to first frame, idle RSS with dev-cluster, idle RSS with 10k fake rows. Route: delegated verification.
- [ ] T5 — Paul's visual QA: compare with the Freelens Pods view and judge scroll smoothness. Route: human.

## Acceptance criteria
- `cargo test --workspace` green; `cargo build --release` produces an arm64 Mach-O binary.
- With `dev-cluster`, the table lists the same pods as `kubectl get pods -A` and updates live when a pod changes.
- Targets (goals, not facts): binary < 40 MB, idle RSS < 150 MB with one cluster, cold start < 0.5 s, smooth scroll with 10k rows.

## Checks
- `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo build --release`.

## Progress / evidence
- 2026-10-23: repo created at `<repo>` (`git init`, no commits). Rust toolchain updated to 1.99.1. Only kube context available: `dev-cluster`.

- 2026-10-23 (T1–T4, route: delegated — writer trigger: 2+ non-trivial files; one bounded writer):
  - Stack resolved: `gpui-kit =0.8.0` (`default-features = false, features = ["component"]`, pins `gpui-pre =0.3.9`), `kube`/`kube-runtime` 4.2.0, `k8s-openapi` 0.28.0 (`v1_33`), `tokio` current-thread runtime on a background thread. `cargo tree --target aarch64-apple-darwin`: 463 crates, no webview/wry/tauri/gpui-pre-web/wasm-bindgen.
  - T1 RED: `cargo test -p natikube-core` with tests written first → 4 test targets failed to compile (E0432 unresolved imports). GREEN: 32 tests pass (age 10, status 15, table 5, source 2).
  - T2 RED: `cargo test -p natikube-k8s` → 1 target failed to compile (E0432). GREEN: 8 fixture tests pass (Running/ReplicaSet, Pending, CrashLoopBackOff, DaemonSet + native sidecar, Terminating; watcher event assembly).
  - T3: GPUI Kit `DataTable` view, events via `futures` mpsc channel, redraws coalesced (≥ 80 ms between event-driven redraws), Age refreshed every 1 s. No meaningful RED for UI; verified by measurement runs.
  - Checks: `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok (only a dependency future-incompat note for `block 0.1.6`); `cargo test --workspace` 43 passed, 0 failed; `cargo build --release` ok.
  - T4 (`scripts/measure.sh dev-cluster 10000`): binary Mach-O arm64, 13.4 MB (target < 40 MB). First frame `--fake-rows 10000`: 922 / 269 / 261 ms (first run cold after build). First frame `--context dev-cluster`: 268 / 274 / 259 ms; first frame with pods: 1027 / 1170 / 1030 ms (target cold start < 0.5 s met for window; data arrival ~1 s is cluster round-trip). Idle RSS after 10 s: dev-cluster 106.7 MB, 10k fake rows 108.3 MB (target < 150 MB). Pod list parity with `kubectl get pods -A`: 17 vs 17, identical.
  - Not done: `opt-level = "s"` size comparison (optional, skipped); live updates and scroll smoothness are T5 (human).

## Next step
T5: Paul runs `./target/release/natikube --context dev-cluster` (and `--fake-rows 10000`), compares with the Freelens Pods view, judges live updates and scroll smoothness, then decides go/no-go and whether to commit F0.
