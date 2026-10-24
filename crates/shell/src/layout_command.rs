//! Command appearance: top bar (cluster pill, ⌘K search, icons), a sidebar
//! with the cluster list and grouped navigation, the content inset in a
//! rounded card, and a footer with the dock tabs and connection status.

use gpui_kit::component::button::Button;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use natikube_core::AppCommand;
use natikube_ui_kit::{ActiveTokens as _, Tokens, status_dot};

use crate::chrome::{NavStyle, state_tone};
use crate::shell_view::ShellView;

impl ShellView {
    pub(crate) fn render_command(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let name = self
            .active_info()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "No cluster".to_string());
        let tone = state_tone(&self.active_state());

        let top_bar = h_flex()
            .w_full()
            .h(px(52.))
            .flex_none()
            .px_4()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(status_dot(tone, cx))
                    .child(self.cluster_dropdown(
                        "command-cluster",
                        format!("{name}  \u{25be}"),
                        cx,
                    )),
            )
            .child(
                div().flex_1().flex().justify_center().child(
                    div().w_full().max_w(px(560.)).child(
                        self.palette_trigger(
                            "command-search",
                            "Jump to any resource or run a command",
                            &tokens,
                            cx,
                        )
                        .w_full(),
                    ),
                ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("command-terminal")
                            .outline()
                            .label(">_")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispatch(AppCommand::OpenLocalShell, window, cx)
                            })),
                    )
                    .child(self.alerts.bell.clone())
                    .child(self.appearance_dropdown("command-appearance", "Aa", cx)),
            );

        let style = NavStyle {
            squares: false,
            upper_captions: true,
            width: 240. * tokens.scale,
            background: p.background,
            selected_bg: p.panel_raised,
            selected_fg: p.text,
            text: p.text,
            muted: p.text_muted,
            hover: p.panel,
            border: p.border,
            radius: tokens.radii.md,
        };
        let sidebar = self.nav_sidebar(&style, cx);
        let content = self.render_content(false, cx);
        let footer = self.command_footer(&tokens, cx);

        v_flex()
            .size_full()
            .bg(p.background)
            .text_color(p.text)
            .font_family(tokens.fonts.ui.clone())
            .text_size(tokens.font_size)
            .child(top_bar)
            .child(
                h_flex().flex_1().min_h_0().child(sidebar).child(
                    div().flex_1().min_w_0().h_full().p_4().child(
                        div()
                            .size_full()
                            .rounded(tokens.radii.lg)
                            .border_1()
                            .border_color(p.border)
                            .overflow_hidden()
                            .child(content),
                    ),
                ),
            )
            .child(footer)
            .into_any_element()
    }

    /// Footer: dock tabs on the left, notices and connection status on the
    /// right.
    fn command_footer(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let p = tokens.palette;
        let tabs: Vec<AnyElement> = self
            .dock_tabs
            .iter()
            .enumerate()
            .map(|(ix, tab)| {
                let title = self.dock_title(tab, cx);
                let shown = self.dock.is_open() && self.dock.active() == Some(ix);
                div()
                    .id(("command-dock", ix))
                    .cursor_pointer()
                    .text_color(if shown { p.text } else { p.text_muted })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.dock.is_open() && this.dock.active() == Some(ix) {
                            this.dock.toggle();
                        } else {
                            this.dock.activate(ix);
                            if !this.dock.is_open() {
                                this.dock.toggle();
                            }
                        }
                        cx.notify();
                    }))
                    .child(title)
                    .into_any_element()
            })
            .collect();
        let state = self.active_state();
        let status = match self.active_info() {
            Some(info) => format!("{} · {}", state.label(), info.name),
            None => state.label(),
        };
        h_flex()
            .w_full()
            .h(px(32.))
            .flex_none()
            .px_4()
            .gap_4()
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .text_xs()
            .text_color(p.text_muted)
            .children(tabs)
            .child(
                div()
                    .id("command-new-terminal")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dispatch(AppCommand::OpenLocalShell, window, cx)
                    }))
                    .child("+ Terminal"),
            )
            .child(div().flex_1())
            .children(self.status_messages(tokens, cx))
            .child(status)
            .into_any_element()
    }
}
