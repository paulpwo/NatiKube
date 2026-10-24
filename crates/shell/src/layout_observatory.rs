//! Observatory appearance: a labeled icon rail with the cluster button,
//! summary cards, kind chips and the workloads below (grouped by namespace by
//! the resources view). Cards show only data the shell really has.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::AppCommand;
use natikube_ui_kit::{ActiveTokens as _, Tokens};

use crate::chrome::state_tone;
use crate::nav_layout::{chips_for, rail_active, rail_items};
use crate::shell_view::ShellView;

fn glyph(label: &str) -> &'static str {
    match label {
        "Health" => "\u{223f}",
        "Workloads" => "\u{25a6}",
        "Network" => "\u{21c4}",
        "Config" => "\u{2630}",
        "Storage" => "\u{25a4}",
        _ => "\u{25f7}",
    }
}

impl ShellView {
    pub(crate) fn render_observatory(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let view = self.nav.view().clone();
        let sections = self.nav.sections();

        let initials = self
            .active_info()
            .map(|c| c.initials())
            .unwrap_or_else(|| "?".to_string());
        let rail_buttons: Vec<AnyElement> = rail_items(sections)
            .into_iter()
            .enumerate()
            .map(|(ix, item)| {
                let active = rail_active(&item, &view, sections);
                let target = item.view.clone();
                let hover = p.panel_raised;
                v_flex()
                    .id(("observatory-rail", ix))
                    .w(px(64.))
                    .py_2()
                    .gap_1()
                    .items_center()
                    .rounded(tokens.radii.lg)
                    .cursor_pointer()
                    .text_size(px(11.))
                    .when(active, |this| {
                        this.bg(p.accent)
                            .text_color(p.background)
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!active, |this| {
                        this.text_color(p.text_muted).hover(move |s| s.bg(hover))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dispatch(AppCommand::Navigate(target.clone()), window, cx);
                    }))
                    .child(div().text_size(px(18.)).child(glyph(item.label)))
                    .child(item.label)
                    .into_any_element()
            })
            .collect();
        let rail = v_flex()
            .id("observatory-rail")
            .h_full()
            .w(px(76.))
            .flex_none()
            .py_3p5()
            .gap_1p5()
            .items_center()
            .border_r_1()
            .border_color(p.border)
            .child(div().w(px(52.)).mb_2p5().child(self.cluster_dropdown(
                "observatory-cluster",
                initials,
                cx,
            )))
            .children(rail_buttons)
            .child(div().flex_1())
            .child(self.alerts.rail_bell.clone());

        let cluster_name = self
            .active_info()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "No cluster".to_string());
        let namespaces = self
            .active
            .as_ref()
            .map(|id| self.settings.prefs(id).namespaces.label())
            .unwrap_or_default();
        let header = h_flex()
            .items_baseline()
            .gap_3()
            .child(
                div()
                    .text_size(px(22.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(cluster_name),
            )
            .child(div().text_color(p.text_muted).child(format!(
                "{} · {}",
                view.title(),
                namespaces
            )))
            .child(div().flex_1())
            .child(
                div().w(px(280.)).child(
                    self.palette_trigger(
                        "observatory-search",
                        "Jump to a resource or command",
                        &tokens,
                        cx,
                    )
                    .w_full(),
                ),
            );

        let state = self.active_state();
        let count = self.active_count(cx);
        let source = self
            .active_info()
            .and_then(|c| c.source_file_name())
            .unwrap_or_else(|| "default".to_string());
        let cards = h_flex()
            .gap_3()
            .child(card(
                view.title(),
                if count > 0 {
                    count.to_string()
                } else {
                    "\u{2014}".into()
                },
                (count > 0).then(|| "loaded".to_string()),
                p.ok,
                &tokens,
            ))
            .child(card(
                "Connection".into(),
                state.label(),
                None,
                state_tone(&state).color(cx),
                &tokens,
            ))
            .child(card(
                "Namespaces".into(),
                namespaces,
                None,
                p.text_muted,
                &tokens,
            ))
            .child(card(
                "Kubeconfig".into(),
                source,
                None,
                p.text_muted,
                &tokens,
            ));

        let chips: Vec<AnyElement> = chips_for(sections, &view)
            .into_iter()
            .enumerate()
            .map(|(ix, chip)| {
                let active = chip.view == view;
                let target = chip.view.clone();
                div()
                    .id(("observatory-chip", ix))
                    .h(px(30.))
                    .px_3p5()
                    .flex()
                    .items_center()
                    .rounded(tokens.radii.md)
                    .border_1()
                    .cursor_pointer()
                    .border_color(if active { p.accent } else { p.border })
                    .bg(if active {
                        p.accent.opacity(0.16)
                    } else {
                        p.panel.opacity(0.0)
                    })
                    .text_color(p.text)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dispatch(AppCommand::Navigate(target.clone()), window, cx);
                    }))
                    .child(chip.label)
                    .into_any_element()
            })
            .collect();

        // Resource lists draw their own heading, health cards and kind chips
        // (the canvas has exactly one set), so the shell adds them only for
        // the overview pages.
        let lists_own_header = matches!(view, natikube_core::ViewId::Resource(_));
        let content = self.render_content(false, cx);
        let status = self.render_status_bar(cx);

        h_flex()
            .size_full()
            .bg(p.background)
            .text_color(p.text)
            .font_family(tokens.fonts.ui.clone())
            .text_size(tokens.font_size)
            .child(rail)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .when(!lists_own_header, |this| this.px_6().py_5().gap_4())
                            .when(!lists_own_header, |this| this.child(header).child(cards))
                            .when(!lists_own_header && !chips.is_empty(), |this| {
                                this.child(h_flex().flex_wrap().gap_2().children(chips))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .rounded(tokens.radii.lg)
                                    .border_1()
                                    .border_color(p.border)
                                    .overflow_hidden()
                                    .child(content),
                            ),
                    )
                    .child(status),
            )
            .into_any_element()
    }
}

/// One summary card: caption, big value and an optional colored suffix.
fn card(
    caption: String,
    value: String,
    suffix: Option<String>,
    suffix_color: Hsla,
    tokens: &Tokens,
) -> Div {
    let p = tokens.palette;
    v_flex()
        .flex_1()
        .min_w_0()
        .px_4()
        .py_3p5()
        .gap_1p5()
        .rounded(tokens.radii.lg)
        .border_1()
        .border_color(p.border)
        .bg(p.panel)
        .child(div().text_xs().text_color(p.text_muted).child(caption))
        .child(
            h_flex()
                .items_baseline()
                .gap_2()
                .child(
                    div()
                        .truncate()
                        .text_size(px(22.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(value),
                )
                .when_some(suffix, |this, suffix| {
                    this.child(div().text_color(suffix_color).child(suffix))
                }),
        )
}
