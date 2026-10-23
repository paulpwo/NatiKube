use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_logs::LogsView;
use natikube_terminal::TerminalView;

use crate::shell_view::ShellView;

/// One tab of the bottom dock.
pub(crate) enum DockTab {
    Logs(Entity<LogsView>),
    Terminal(Entity<TerminalView>),
    /// The singleton port-forwards list (its entity lives in the shell).
    PortForwards,
}

impl ShellView {
    pub(crate) fn dock_title(&self, tab: &DockTab, cx: &App) -> SharedString {
        match tab {
            DockTab::Logs(view) => view.read(cx).title(),
            DockTab::Terminal(view) => view.read(cx).title(),
            DockTab::PortForwards => self.port_forwards.read(cx).title(),
        }
    }

    pub(crate) fn render_dock(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let active = self.dock.active();
        let tabs: Vec<_> = self
            .dock_tabs
            .iter()
            .enumerate()
            .map(|(ix, tab)| {
                let selected = active == Some(ix);
                let title = self.dock_title(tab, cx);
                h_flex()
                    .id(("dock-tab", ix))
                    .h_full()
                    .px_3()
                    .gap_2()
                    .items_center()
                    .cursor_pointer()
                    .text_sm()
                    .border_b_2()
                    .border_color(if selected {
                        theme.primary
                    } else {
                        theme.sidebar.opacity(0.0)
                    })
                    .text_color(if selected {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dock.activate(ix);
                        cx.notify();
                    }))
                    .child(title)
                    .child(
                        div()
                            .id(("dock-close", ix))
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .hover(|s| s.text_color(gpui_kit::white()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_dock_tab(ix, cx);
                            }))
                            .child("✕"),
                    )
            })
            .collect();

        let body = active
            .and_then(|ix| self.dock_tabs.get(ix))
            .map(|tab| match tab {
                DockTab::Logs(view) => view.clone().into_any_element(),
                DockTab::Terminal(view) => view.clone().into_any_element(),
                DockTab::PortForwards => self.port_forwards.clone().into_any_element(),
            });

        v_flex()
            .size_full()
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .w_full()
                    .h_9()
                    .flex_none()
                    .bg(theme.sidebar)
                    .border_b_1()
                    .border_color(theme.border)
                    .children(tabs)
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("dock-hide")
                            .px_3()
                            .cursor_pointer()
                            .text_color(theme.muted_foreground)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dock.toggle();
                                cx.notify();
                            }))
                            .child("▾"),
                    ),
            )
            .when_some(body, |this, body| {
                this.child(div().flex_1().min_h_0().child(body))
            })
            .into_any_element()
    }
}
