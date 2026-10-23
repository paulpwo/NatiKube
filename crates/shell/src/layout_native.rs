//! macOS Native appearance: source-list sidebar, a toolbar with title and
//! subtitle, and a right inspector that hosts the detail panel. Colors follow
//! the macOS light/dark setting through the Native tokens.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use natikube_ui_kit::ActiveTokens as _;

use crate::chrome::NavStyle;
use crate::shell_view::ShellView;

impl ShellView {
    pub(crate) fn render_native(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;

        let title = self.nav.view().title();
        let subtitle = match self.active_info() {
            Some(info) => {
                let namespaces = self.settings.prefs(&info.id).namespaces.label();
                format!("{} · {}", info.name, namespaces)
            }
            None => "No cluster selected".to_string(),
        };

        let toolbar = h_flex()
            .w_full()
            .h(px(60.))
            .flex_none()
            .px_5()
            .gap_4()
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.text_muted)
                            .child(subtitle),
                    ),
            )
            .child(div().flex_1())
            .child(
                div().w(px(240.)).child(
                    self.palette_trigger("native-search", "Search", &tokens, cx)
                        .w_full(),
                ),
            );

        let style = NavStyle {
            squares: true,
            upper_captions: false,
            width: 240. * tokens.scale,
            background: p.panel,
            selected_bg: p.accent,
            selected_fg: gpui_kit::white(),
            text: p.text,
            muted: p.text_muted,
            hover: p.panel_raised,
            border: p.border,
            radius: tokens.radii.md,
        };
        let sidebar = self.nav_sidebar(&style, cx);
        let content = self.render_content(true, cx);
        let status = self.render_status_bar(cx);

        h_flex()
            .size_full()
            .bg(p.background)
            .text_color(p.text)
            .font_family(tokens.fonts.ui.clone())
            .text_size(tokens.font_size)
            .child(sidebar)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(toolbar)
                    .child(div().flex_1().min_h_0().child(content))
                    .child(status),
            )
            .into_any_element()
    }
}
