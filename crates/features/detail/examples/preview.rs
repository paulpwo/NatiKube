//! Manual preview of the detail panel on its own (read-only use only).
//!
//! cargo run -p natikube-detail --example preview -- <context> <kind> <namespace|-> <name>
//! e.g. ... -- dev-cluster pods kube-system coredns-xxxx
//!
//! The panel only watches the object and lists its events; do not click the
//! action buttons against a real cluster you do not own.

use gpui_kit::*;
use natikube_core::{AppCommand, ClusterInfo, ResourceRef, kinds};
use natikube_detail::DetailPanel;
use natikube_kube_client::{ClusterConnection, runtime_handle};
use natikube_ui_kit::apply_theme;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [context, kind, namespace, name] = args.as_slice() else {
        eprintln!("usage: preview <context> <kind> <namespace|-> <name>");
        std::process::exit(2);
    };
    let info = ClusterInfo::from_context(context);
    let conn = runtime_handle()
        .block_on(ClusterConnection::connect(&info))
        .expect("cluster connection failed");
    let kind = kinds::find(kind).expect("unknown kind");
    let namespace = (namespace != "-").then(|| namespace.clone());
    let target = ResourceRef::new(info.id.clone(), kind, namespace, name.clone());

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        apply_theme(cx);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(520.), px(860.)), cx)),
            ..WindowOptions::default()
        };
        gpui_kit::open_window(options, cx, move |window, cx| {
            let panel = cx.new(|cx| DetailPanel::new(conn, window, cx));
            panel.update(cx, |p, cx| p.show(target, cx));
            cx.subscribe(&panel, |_, event: &AppCommand, _| {
                eprintln!("event: {event:?}")
            })
            .detach();
            panel
        })
        .expect("window");
        cx.activate(true);
    });
}
