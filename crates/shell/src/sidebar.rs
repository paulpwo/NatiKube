use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::AppCommand;
use natikube_ui_kit::ActiveTokens as _;

use crate::shell_view::ShellView;

/// Sidebar width in pixels.
pub(crate) const SIDEBAR_WIDTH: f32 = 232.;

impl ShellView {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let current = self.nav.section_index();
        let mut rows: Vec<AnyElement> = Vec::new();

        for (six, section) in self.nav.sections().iter().enumerate() {
            let selected = current == Some(six);
            let expanded = section.is_group() || section.dynamic;
            let open = expanded && self.nav.is_expanded(six);
            let landing = section.landing();
            let label = section.label;
            rows.push(
                h_flex()
                    .id(("section", six))
                    .w_full()
                    .h_8()
                    .px_3()
                    .gap_2()
                    .items_center()
                    .cursor_pointer()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if selected {
                        theme.sidebar_accent_foreground
                    } else {
                        theme.sidebar_foreground
                    })
                    .when(selected, |this| this.bg(theme.sidebar_accent))
                    .hover(|s| s.bg(theme.sidebar_accent.opacity(0.5)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        match &landing {
                            Some(view) if !selected || !expanded => {
                                this.dispatch(AppCommand::Navigate(view.clone()), window, cx);
                            }
                            _ => {}
                        }
                        if expanded {
                            this.nav.toggle_section(six);
                        }
                        cx.notify();
                    }))
                    .child(div().flex_1().child(label))
                    .when(expanded, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(if open { "▾" } else { "▸" }),
                        )
                    })
                    .into_any_element(),
            );
            if open {
                if section.dynamic {
                    let definitions = section.entries.iter().enumerate().filter(|(_, e)| {
                        !self
                            .nav
                            .custom_groups()
                            .iter()
                            .any(|(_, list)| list.iter().any(|g| g.view == e.view))
                    });
                    for (eix, entry) in definitions {
                        rows.push(self.entry_row(six * 1000 + eix, entry, false, cx));
                    }
                    for (gix, (group, entries)) in self.nav.custom_groups().into_iter().enumerate()
                    {
                        let group_open = self.nav.is_group_expanded(&group);
                        let toggle = group.clone();
                        rows.push(
                            h_flex()
                                .id(("crd-group", six * 1000 + gix))
                                .w_full()
                                .h_7()
                                .pl_8()
                                .pr_3()
                                .gap_2()
                                .items_center()
                                .cursor_pointer()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .hover(|s| s.bg(theme.sidebar_accent.opacity(0.5)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.nav.toggle_group(&toggle);
                                    cx.notify();
                                }))
                                .child(div().flex_1().truncate().child(group.clone()))
                                .child(div().text_xs().child(if group_open {
                                    "▾"
                                } else {
                                    "▸"
                                }))
                                .into_any_element(),
                        );
                        if group_open {
                            for (eix, entry) in entries.into_iter().enumerate() {
                                rows.push(self.entry_row(
                                    (six + 1) * 100_000 + gix * 1000 + eix,
                                    entry,
                                    true,
                                    cx,
                                ));
                            }
                        }
                    }
                } else {
                    for (eix, entry) in section.entries.iter().enumerate() {
                        rows.push(self.entry_row(six * 1000 + eix, entry, false, cx));
                    }
                }
            }
        }

        v_flex()
            .h_full()
            .w(px(SIDEBAR_WIDTH * cx.tokens().scale))
            .flex_none()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .child(self.header.clone())
            .child(
                v_flex()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py_1()
                    .children(rows),
            )
            .into_any_element()
    }

    fn entry_row(
        &self,
        id: usize,
        entry: &natikube_core::SidebarEntry,
        nested: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let active = self.nav.view() == &entry.view;
        let view = entry.view.clone();
        div()
            .id(("entry", id))
            .w_full()
            .h_7()
            .when(nested, |this| this.pl_12())
            .when(!nested, |this| this.pl_8())
            .pr_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_sm()
            .text_color(if active {
                theme.sidebar_accent_foreground
            } else {
                theme.muted_foreground
            })
            .when(active, |this| this.bg(theme.sidebar_accent))
            .hover(|s| s.bg(theme.sidebar_accent.opacity(0.5)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.dispatch(AppCommand::Navigate(view.clone()), window, cx);
            }))
            .child(div().truncate().child(entry.label.clone()))
            .into_any_element()
    }
}
