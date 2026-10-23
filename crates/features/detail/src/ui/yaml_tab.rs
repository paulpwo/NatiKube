//! The YAML tab: highlighted viewer with an explicit edit / validate / apply flow.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Editor;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use natikube_core::kinds;

use super::panel::DetailPanel;

impl DetailPanel {
    pub(super) fn render_yaml_tab(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, muted, red, yellow) = (
            theme.border,
            theme.muted_foreground,
            theme.red,
            theme.yellow,
        );
        let is_secret = self
            .target
            .as_ref()
            .is_some_and(|t| t.kind == kinds::SECRET);
        let busy = self.busy.is_some();

        let mut bar = h_flex()
            .w_full()
            .flex_none()
            .gap_1()
            .px_3()
            .py_2()
            .items_center()
            .flex_wrap()
            .border_b_1()
            .border_color(border);

        if self.editing {
            bar = bar
                .child(
                    Button::new("yaml-save")
                        .label("Save")
                        .small()
                        .primary()
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.save_yaml(cx))),
                )
                .child(
                    Button::new("yaml-cancel")
                        .label("Cancel")
                        .small()
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_edit(cx))),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Save validates with a server dry run before applying"),
                );
        } else {
            bar = bar.child(
                Button::new("yaml-edit")
                    .label("Edit")
                    .small()
                    .disabled(!self.yaml_editable() || self.deleted)
                    .on_click(cx.listener(|this, _, _, cx| this.begin_edit(cx))),
            );
            if is_secret {
                bar = bar.child(
                    Button::new("yaml-reveal")
                        .label(if self.reveal_yaml {
                            "Hide values"
                        } else {
                            "Reveal values"
                        })
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_reveal_yaml(cx))),
                );
            }
            bar = bar.child(
                Button::new("yaml-managed")
                    .label("Managed fields")
                    .small()
                    .toggled(self.show_managed)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_managed(cx))),
            );
            if !self.yaml_editable() {
                bar = bar.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Secret values are hidden; reveal them to edit"),
                );
            }
        }

        let mut col = v_flex().size_full().child(bar);
        if self.server_changed {
            col = col.child(
                div()
                    .w_full()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(yellow)
                    .child("The object changed on the server while you were editing; saving may conflict."),
            );
        }
        if let Some(message) = &self.validation {
            col = col.child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(red)
                    .border_b_1()
                    .border_color(border)
                    .child(message.clone()),
            );
        }
        col.child(
            div().flex_1().min_h_0().child(
                Editor::new(&self.yaml)
                    .readonly(!self.editing)
                    .bordered(false)
                    .h(relative(1.)),
            ),
        )
    }
}
