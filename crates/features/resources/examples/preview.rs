//! Developer preview of the resource browser without the shell.
//!
//! ```text
//! cargo run -p natikube-resources --example preview -- --fake 500
//! cargo run -p natikube-resources --example preview -- --context dev-cluster --view deployments
//! ```
//!
//! Views: `cluster`, `workloads`, or any kind plural/name (`pods`, `nodes`, ...).
//! `--appearance <standard|command|native|observatory>` picks the look.
//! `--quit-after <seconds>` closes the window by itself (smoke tests).

use std::time::Duration;

use gpui_kit::*;
use natikube_core::{AppCommand, Appearance, ClusterInfo, NamespaceFilter, ViewId, kinds};
use natikube_kube_client::{ClusterConnection, runtime_handle};
use natikube_resources::ResourcesView;
use natikube_ui_kit::set_appearance;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn parse_view(name: &str) -> ViewId {
    match name {
        "cluster" => ViewId::ClusterOverview,
        "workloads" => ViewId::WorkloadsOverview,
        other => ViewId::Resource(kinds::find(other).unwrap_or(kinds::POD)),
    }
}

fn main() {
    let view = parse_view(&arg("--view").unwrap_or_else(|| "pods".into()));
    let fake: Option<usize> = arg("--fake").and_then(|n| n.parse().ok());
    let connection = if fake.is_none() {
        let context = arg("--context").expect("pass --context <name> or --fake <count>");
        let info = ClusterInfo::from_context(&context);
        Some(
            runtime_handle()
                .block_on(ClusterConnection::connect(&info))
                .expect("cannot connect"),
        )
    } else {
        None
    };
    let appearance = arg("--appearance")
        .and_then(|slug| Appearance::from_slug(&slug))
        .unwrap_or_default();
    let quit_after: Option<u64> = arg("--quit-after").and_then(|n| n.parse().ok());

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        set_appearance(appearance, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1360.), px(860.)), cx)),
            ..WindowOptions::default()
        };
        gpui_kit::open_window(options, cx, move |window, cx| {
            let resources = cx.new(|cx| match connection {
                Some(conn) => ResourcesView::new(conn, view, NamespaceFilter::All, window, cx),
                None => ResourcesView::fake(fake.unwrap_or(100), view, window, cx),
            });
            cx.subscribe(&resources, |_, command: &AppCommand, _| {
                eprintln!("command: {command:?}");
            })
            .detach();
            resources
        })
        .expect("window");
        cx.activate(true);
        if let Some(seconds) = quit_after {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(seconds))
                    .await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}
