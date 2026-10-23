use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::{AppCommand, ClusterId, ClusterInfo, ConnectionState};
use natikube_ui_kit::{ActiveTokens as _, Tone, status_dot};

use crate::domain::{ClusterList, avatar_hue};

/// Width of the rail in pixels.
pub const RAIL_WIDTH: f32 = 60.;

/// The left cluster rail (Freelens hotbar): one initials avatar per cluster,
/// active highlight and a connection-health dot. Emits
/// [`AppCommand::SelectCluster`] when an avatar is clicked.
pub struct ClusterRail {
    list: ClusterList,
}

impl EventEmitter<AppCommand> for ClusterRail {}

impl ClusterRail {
    pub fn new(clusters: Vec<ClusterInfo>, active: Option<ClusterId>) -> Self {
        let mut list = ClusterList::new(clusters);
        list.set_active(active);
        Self { list }
    }

    pub fn set_clusters(&mut self, clusters: Vec<ClusterInfo>, cx: &mut Context<Self>) {
        self.list.set_clusters(clusters);
        cx.notify();
    }

    pub fn set_active(&mut self, active: Option<ClusterId>, cx: &mut Context<Self>) {
        self.list.set_active(active);
        cx.notify();
    }

    pub fn set_state(
        &mut self,
        cluster: ClusterId,
        state: ConnectionState,
        cx: &mut Context<Self>,
    ) {
        self.list.set_state(cluster, state);
        cx.notify();
    }
}

/// Initials avatar with a color derived from the cluster name. The active
/// cluster gets the accent ring and a brighter fill.
fn avatar(
    ix: usize,
    info: &ClusterInfo,
    active: bool,
    health: Option<Tone>,
    cx: &App,
) -> Stateful<Div> {
    let tokens = cx.tokens();
    let palette = tokens.palette;
    // Standard: lg (6) + 2 = the 8 px avatar corner it always had.
    let radius = tokens.radii.lg + px(2.);
    let hue = avatar_hue(&info.name);
    let bg = hsla(hue, 0.45, if active { 0.42 } else { 0.30 }, 1.);
    let ring = if active { palette.accent } else { bg };
    let hover = hsla(hue, 0.45, 0.38, 1.);
    div()
        .id(("cluster", ix))
        .relative()
        .flex_none()
        .size(px(40.))
        .rounded(radius)
        .border_2()
        .border_color(ring)
        .bg(bg)
        .text_color(white())
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .when(!active, move |this| this.hover(move |s| s.bg(hover)))
        .child(SharedString::from(info.initials()))
        .when_some(health, |this, tone| {
            this.child(
                status_dot(tone, cx)
                    .absolute()
                    .bottom(px(-3.))
                    .right(px(-3.))
                    .border_2()
                    .border_color(palette.panel),
            )
        })
}

fn health(state: &ConnectionState) -> Option<Tone> {
    match state {
        ConnectionState::Connected => Some(Tone::Ok),
        ConnectionState::Connecting => Some(Tone::Warning),
        ConnectionState::Failed(_) => Some(Tone::Error),
        ConnectionState::Disconnected => None,
    }
}

/// `name (state)`, plus the source kubeconfig file and any other file that
/// declares the same context.
fn tooltip_text(info: &ClusterInfo, state: &ConnectionState) -> SharedString {
    let mut text = format!("{} ({})", info.name, state.label());
    if let Some(file) = info.source_file_name() {
        text.push_str(&format!("\nFrom {file}"));
    }
    let others: Vec<String> = info
        .also_in
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    if !others.is_empty() {
        text.push_str(&format!("\nAlso in {}", others.join(", ")));
    }
    if !info.aliases.is_empty() {
        text.push_str(&format!("\nAlso known as {}", info.aliases.join(", ")));
    }
    text.into()
}

impl Render for ClusterRail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.tokens().palette;
        let active = self.list.active().cloned();
        let items: Vec<_> = self
            .list
            .clusters()
            .iter()
            .enumerate()
            .map(|(ix, info)| {
                let id = info.id.clone();
                let is_active = active.as_ref() == Some(&id);
                let state = self.list.state(&id);
                avatar(ix, info, is_active, health(&state), cx)
                    .tooltip({
                        let text = tooltip_text(info, &state);
                        move |window, cx| Tooltip::new(text.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(AppCommand::SelectCluster(id.clone()));
                    }))
            })
            .collect();

        v_flex()
            .h_full()
            .w(px(RAIL_WIDTH))
            .flex_none()
            .items_center()
            .gap_3()
            .py_3()
            .bg(palette.panel)
            .border_r_1()
            .border_color(palette.border)
            .children(items)
            .child(add_button(cx))
    }
}

/// The "+" at the end of the rail: opens the Add kubeconfig panel.
fn add_button(cx: &mut Context<ClusterRail>) -> impl IntoElement {
    let tokens = cx.tokens();
    let (palette, radius) = (tokens.palette, tokens.radii.lg + px(2.));
    div()
        .id("add-kubeconfig")
        .flex_none()
        .size(px(40.))
        .rounded(radius)
        .border_1()
        .border_color(palette.border)
        .text_color(palette.text_muted)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |s| s.bg(palette.panel_raised).text_color(palette.text))
        .child("+")
        .tooltip(|window, cx| Tooltip::new("Add kubeconfig\u{2026}").build(window, cx))
        .on_click(cx.listener(|_, _, _, cx| cx.emit(AppCommand::AddKubeconfig)))
}
