<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/icon/natikube-wordmark-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/icon/natikube-wordmark-light.svg">
  <img alt="NatiKube" src="assets/icon/natikube-wordmark-light.svg" width="420">
</picture>

**A fast, 100% native Kubernetes desktop app. Rust and GPUI: no Electron, no webview, no bundled kubectl.**

[![Status: alpha](https://img.shields.io/badge/status-alpha-orange.svg)](#status)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platforms: macOS | Windows](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey.svg)](#install)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org)
[![UI: GPUI](https://img.shields.io/badge/UI-GPUI-8A63D2.svg)](https://www.gpui.rs)

<img src="docs/screenshots/command.png" alt="NatiKube, Command appearance, showing the pods of a demo cluster" width="900">

</div>

> [!WARNING]
> **NatiKube is in early alpha.** It is under active development: expect rough edges, missing features and breaking changes between releases. Please [open an issue](https://github.com/paulpwo/NatiKube/issues) when something breaks.

## Gallery

Four appearances, switchable live. Every screenshot below was taken with `--demo`, a fictional cluster built into the app.

| | |
| :---: | :---: |
| <img src="docs/screenshots/standard.png" alt="Standard appearance"><br>**Standard**: the familiar cluster rail, sidebar and tabs. | <img src="docs/screenshots/command.png" alt="Command appearance"><br>**Command**: top bar, stacked names, status pills and usage bars. |
| <img src="docs/screenshots/native-light.png" alt="macOS Native appearance, light"><br>**Native (light)**: source-list sidebar and zebra rows. | <img src="docs/screenshots/native-dark.png" alt="macOS Native appearance, dark"><br>**Native (dark)**: follows the system light or dark setting. |
| <img src="docs/screenshots/observatory.png" alt="Observatory appearance"><br>**Observatory**: icon rail, KPI cards, pods grouped by namespace. | <img src="docs/screenshots/settings.png" alt="Settings window"><br>**Settings**: template, density, fonts, zoom, terminal, logs and data. |

## Features

- **Fast and light.** A single native binary of about 20 MB. The first frame shows in a few hundred milliseconds and lists are virtualized, so a 10,000-row list stays smooth.
- **Browse everything.** Workloads, nodes, namespaces, events, config, network, storage, access control and Custom Resources discovered from the cluster's CRDs. Live updates through watches, with CPU and memory from metrics.
- **Detail panel.** Overview, YAML editor with apply, scale and delete, events and Secret keys.
- **Logs.** Streaming pod logs with tail, timestamps and wrapping.
- **Terminals.** `exec` into a container, or open a local shell (PowerShell, `cmd` or your Unix shell) in the dock.
- **Port forwarding.** Start, list and stop forwards, and open them in the browser.
- **Command palette.** `Ctrl/Cmd+K` jumps to any resource, view or command.
- **Many clusters.** Reads every kubeconfig it finds, folds duplicates (including EKS alias and ARN twins) and imports the cluster list from Freelens.
- **Alerts and notifications.** Pick the signals you care about per cluster (new pod, failing pod, cluster unreachable and more), see them in a bell with a local history, and optionally get operating system notifications. All off until you turn them on.
- **Four appearances and a Settings window**, all applied live and saved as plain JSON.

## Install

Download the latest build from [GitHub Releases](../../releases/latest). The builds are not code-signed or notarized (no paid certificates), so your OS asks for confirmation the first time.

### macOS (Apple Silicon)

1. Download `NatiKube-<version>-macos-arm64.dmg`, open it and drag **NatiKube** to **Applications**.
2. Remove the quarantine flag:

   ```sh
   xattr -dr com.apple.quarantine /Applications/NatiKube.app
   ```

3. On macOS 15 and later, if the app is still blocked: **System Settings > Privacy & Security**, scroll to the message about NatiKube and choose **Open Anyway**.

### Windows (x64)

1. Download `NatiKube-<version>-windows-x64.zip` and extract it.
2. Run `natikube.exe`. If SmartScreen shows "Windows protected your PC", choose **More info**, then **Run anyway**.

### Verify your download

Each file has a matching `.sha256`. Put both in the same folder and run:

```sh
shasum -a 256 -c NatiKube-<version>-macos-arm64.dmg.sha256      # macOS
```

```powershell
(Get-FileHash NatiKube-<version>-windows-x64.zip).Hash.ToLower() # Windows: compare with the .sha256 file
```

### Homebrew

A Homebrew cask is **coming soon**. A template lives in [`packaging/homebrew`](packaging/homebrew/natikube.rb).

## Status

NatiKube is **alpha** software. Releases are published as pre-releases until the first stable version.

| Platform | Status |
| --- | --- |
| macOS (Apple Silicon) | Alpha |
| Windows (x64) | Alpha, less tested |
| Linux | Planned |

## Build from source

You need a recent stable Rust toolchain (see `rust-toolchain.toml`).

```sh
cargo build --release
./target/release/natikube                    # your current kubeconfig context
./target/release/natikube --demo             # a fictional cluster, no kubeconfig needed
```

Packaging: `scripts/package-macos.sh` builds the signed-ad-hoc `.app` and a DMG, `scripts/package-windows.ps1` builds the ZIP.

Useful flags: `--context <name>`, `--demo`, `--appearance <standard|command|native|observatory>`, `--native-mode <light|dark>`, `--open-settings`, `--dump-clusters`, `--dump <kind>`, `--fake-rows <N>`, `--exit-after-first-frame`. Run `natikube --help` for the list. Appearance flags apply to one run and are never saved.

On Windows release builds the app is a GUI-subsystem executable, so CLI flags (`--help`, `--dump-clusters`, `--dump <kind>`, ...) print nothing to the terminal. Use a debug build (`cargo run -- --dump-clusters`) when you need their output.

Checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Updates and privacy

NatiKube does not phone home, with one exception: when it starts it makes **one read-only request to `api.github.com`** (`GET /repos/paulpwo/NatiKube/releases`) to see whether a newer release exists. If there is one, a dismissible dialog offers it as an optional update: **Download** opens the release page in your browser, **Later** asks again next launch, **Skip this version** stays quiet until a newer one appears. Nothing is downloaded or installed automatically.

The request carries only a `User-Agent: NatiKube/<version>` header (GitHub requires one); no identifiers, no telemetry, and any failure (offline, rate limit) is silent. To turn it off, untick **Settings > Updates > Check for updates on startup**, set `updates.check_on_startup` to `false` in `settings.json`, or launch with the environment variable `NATIKUBE_NO_UPDATE_CHECK=1`. `--demo` never checks.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Cmd+K` (`Ctrl+K` on Windows) | Command palette |
| `Cmd+,` (`Ctrl+,` on Windows) | Settings |
| `Cmd+Shift+N` (`Ctrl+Shift+N` on Windows) | Notifications (opens the bell panel) |
| `Cmd+M` (`Ctrl+M` on Windows) | Minimize |
| `Cmd+Q` (`Ctrl+Q` on Windows) | Quit |

In terminals, copy and paste are `Cmd+C` / `Cmd+V` on macOS and `Ctrl+Shift+C` / `Ctrl+Shift+V` elsewhere, so plain `Ctrl+C` still interrupts.

## Kubeconfig discovery

NatiKube reads every kubeconfig in `~/.kube/` (top-level files), `$KUBECONFIG` and any files or folders you add in Settings > Clusters. Identical contexts (same server, user and namespace) are folded into one entry, preferring `~/.kube/config`, then `$KUBECONFIG` order, then the other files. EKS contexts show their short cluster name instead of the ARN. **Import from Freelens** adds the kubeconfig paths from Freelens' cluster store. On Windows the folder is `%USERPROFILE%\.kube`.

Settings live in `~/Library/Application Support/NatiKube/settings.json` on macOS and `%APPDATA%\NatiKube\settings.json` on Windows. A file with a syntax error is kept as a `.bak` and never overwritten.

## Alerts and notifications

NatiKube can watch clusters for the things you choose and keep a history of what it saw. **Everything is off by default**: until you enable a rule in Settings > Alerts, no watcher starts for alerts and nothing is recorded. OS notifications are a separate opt-in switch, also off by default.

**Turning it on.** Open Settings > Alerts, pick a cluster, and switch on the rules you want for it. Only clusters with at least one enabled rule are watched, and only while NatiKube is open. Each rule can have a namespace filter, and some have parameters.

| Rule | Fires when |
| --- | --- |
| New pod | A pod appears after monitoring started. Pods that already existed never fire it. |
| Pod failing | A container enters CrashLoopBackOff, Error, ImagePullBackOff, ErrImagePull or CreateContainerConfigError, or is OOMKilled. |
| Pod restarts | A container's restart count grows by N (default 3) within a window (default 10 minutes). |
| Pod pending too long | A pod stays Pending longer than a duration (default 5 minutes). |
| Deployment unavailable | A deployment has unavailable replicas for longer than a duration (default 2 minutes). |
| Node not ready | A node's Ready condition becomes False or Unknown. |
| Cluster unreachable | The cluster API stops answering, and again when it recovers. |
| Warning events | Kubernetes Warning events (FailedScheduling, BackOff, Unhealthy, FailedMount, network sandbox failures and so on), optionally filtered by reason. |

To keep the noise down, objects that are already in a bad state when monitoring starts do not raise "new" alerts (pending-too-long and deployment-unavailable still check them), and repeated alerts for the same object and reason are collapsed for a cooldown (default 5 minutes).

**The bell.** The bell in the header (the left rail in Observatory) shows an unread badge. Its panel lists alerts newest first with severity, cluster, object, message and time. Clicking an alert switches to that cluster and opens the object. You can mark everything read or clear the history. View > Notifications (`Cmd/Ctrl+Shift+N`) opens it too.

**Where the history lives.** In a SQLite file next to your settings: `notifications.sqlite` in the NatiKube config folder (`~/Library/Application Support/NatiKube/` on macOS, `%APPDATA%\NatiKube\` on Windows). By default it keeps 30 days and 5,000 rows; both are adjustable in Settings > Alerts.

**OS notifications.** Settings > Alerts > OS notifications sends each new alert to the system notification center (macOS and Windows). Use **Send test notification** to check it works. If the switch is off, alerts only go to the history.

**Privacy.** Nothing leaves your machine. Watching is read-only (list and watch), history and settings stay in local files, and there is no telemetry or account.

## Architecture

Vertical feature slices over a small core, with no feature depending on another. See [`docs/architecture.md`](docs/architecture.md).

## Acknowledgements

- Built on [GPUI](https://www.gpui.rs) through [GPUI Kit](https://github.com/longbridge/gpui-kit), [kube-rs](https://kube.rs), [Tokio](https://tokio.rs) and [alacritty_terminal](https://github.com/alacritty/alacritty).
- The Command appearance embeds the [Hanken Grotesk](https://fonts.google.com/specimen/Hanken+Grotesk) font under the SIL Open Font License ([`assets/icon/HankenGrotesk-OFL.txt`](assets/icon/HankenGrotesk-OFL.txt)).
- Licenses of every dependency are listed in [`packaging/THIRD-PARTY-NOTICES.md`](packaging/THIRD-PARTY-NOTICES.md), which also ships inside each download.

## License

[MIT](LICENSE) © 2026 [Paul Osinga](https://github.com/paulpwo)
