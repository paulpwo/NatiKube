//! Window chrome shared by the Command, Native and Observatory layouts:
//! cluster and appearance dropdowns, the ⌘K trigger, the cluster list and the
//! captioned navigation sidebar. Colors, radii and fonts come from the active
//! tokens; nothing here hard-codes a palette.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_clusters::avatar_hue;
use natikube_core::{AppCommand, Appearance, ClusterId, ClusterInfo, ConnectionState, ViewId};
use natikube_ui_kit::{ActiveTokens as _, Tokens};

use crate::nav_layout::{NavLink, cluster_tooltip, sidebar_groups};
use crate::shell_view::ShellView;

/// How the shared sidebar looks in one appearance.
pub(crate) struct NavStyle {
    /// Clusters get a colored square (Native) instead of an initials chip.
    pub squares: bool,
    /// Captions are uppercase (Command) instead of semibold sentence case.
    pub upper_captions: bool,
    pub width: f32,
    pub background: Hsla,
    pub selected_bg: Hsla,
    pub selected_fg: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub hover: Hsla,
    pub border: Hsla,
    pub radius: Pixels,
}

/// Menu entry that dispatches `command` to the shell.
fn command_item(
    shell: &Entity<ShellView>,
    label: impl Into<SharedString>,
    checked: bool,
    command: AppCommand,
) -> PopupMenuItem {
    let shell = shell.clone();
    PopupMenuItem::new(label)
        .checked(checked)
        .on_click(move |_, window, cx| {
            let command = command.clone();
            shell.update(cx, |this, cx| this.dispatch(command, window, cx));
        })
}

pub(crate) fn state_tone(state: &ConnectionState) -> natikube_ui_kit::Tone {
    use natikube_ui_kit::Tone;
    match state {
        ConnectionState::Connected => Tone::Ok,
        ConnectionState::Connecting => Tone::Warning,
        ConnectionState::Failed(_) => Tone::Error,
        ConnectionState::Disconnected => Tone::Muted,
    }
}

impl ShellView {
    pub(crate) fn active_info(&self) -> Option<&ClusterInfo> {
        let id = self.active.as_ref()?;
        self.clusters.iter().find(|c| &c.id == id)
    }

    pub(crate) fn active_state(&self) -> ConnectionState {
        self.active
            .as_ref()
            .and_then(|id| self.states.get(id))
            .cloned()
            .unwrap_or_default()
    }

    /// Number of rows in the active list (0 for overviews and no session).
    pub(crate) fn active_count(&self, cx: &App) -> usize {
        self.session
            .as_ref()
            .map(|s| s.resources.read(cx).item_count(cx))
            .unwrap_or(0)
    }

    /// Dropdown listing every cluster plus the kubeconfig actions.
    pub(crate) fn cluster_dropdown(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shell = cx.entity();
        let active = self.active.clone();
        let entries: Vec<(ClusterId, SharedString)> = self
            .clusters
            .iter()
            .map(|c| (c.id.clone(), SharedString::from(c.name.clone())))
            .collect();
        Button::new(id)
            .outline()
            .label(label)
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu;
                if entries.is_empty() {
                    menu = menu.item(PopupMenuItem::label("No clusters found"));
                }
                for (cluster, name) in entries.clone() {
                    let checked = active.as_ref() == Some(&cluster);
                    menu = menu.item(command_item(
                        &shell,
                        name,
                        checked,
                        AppCommand::SelectCluster(cluster),
                    ));
                }
                menu.item(PopupMenuItem::separator())
                    .item(command_item(
                        &shell,
                        "Add kubeconfig…",
                        false,
                        AppCommand::AddKubeconfig,
                    ))
                    .item(command_item(
                        &shell,
                        "Import from Freelens…",
                        false,
                        AppCommand::ImportFreelens,
                    ))
            })
    }

    /// Dropdown to switch the appearance (the same four choices as the menu).
    pub(crate) fn appearance_dropdown(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shell = cx.entity();
        let current = cx.tokens().appearance;
        Button::new(id)
            .ghost()
            .label(label)
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu;
                for appearance in Appearance::all() {
                    menu = menu.item(command_item(
                        &shell,
                        appearance.label(),
                        appearance == current,
                        AppCommand::SetAppearance(appearance),
                    ));
                }
                menu
            })
    }

    /// The search-looking box that opens the ⌘K palette.
    pub(crate) fn palette_trigger(
        &self,
        id: &'static str,
        placeholder: &'static str,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = tokens.palette;
        h_flex()
            .id(id)
            .h(px(32.))
            .px_3()
            .gap_2()
            .items_center()
            .rounded(tokens.radii.md)
            .border_1()
            .border_color(p.border)
            .bg(p.panel)
            .text_color(p.text_muted)
            .cursor_pointer()
            .hover(move |s| s.border_color(p.accent))
            .on_click(
                cx.listener(|this, _, window, cx| {
                    this.dispatch(AppCommand::OpenPalette, window, cx)
                }),
            )
            .child("⌕")
            .child(div().flex_1().truncate().child(placeholder))
            .child(
                div()
                    .px_1p5()
                    .rounded(tokens.radii.sm)
                    .border_1()
                    .border_color(p.border)
                    .text_size(px(11.))
                    .font_family(tokens.fonts.mono.clone())
                    .child(palette_hint(cfg!(target_os = "macos"))),
            )
    }

    /// Notice and error messages for the footer; click dismisses.
    pub(crate) fn status_messages(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if let Some(message) = self.notice.clone() {
            out.push(
                div()
                    .id("footer-notice")
                    .cursor_pointer()
                    .text_color(tokens.palette.text)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.notice = None;
                        cx.notify();
                    }))
                    .child(message)
                    .into_any_element(),
            );
        }
        if let Some(message) = self.error.clone() {
            out.push(
                div()
                    .id("footer-error")
                    .cursor_pointer()
                    .text_color(tokens.palette.error)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.error = None;
                        cx.notify();
                    }))
                    .child(message)
                    .into_any_element(),
            );
        }
        out
    }

    /// One cluster row: badge, name, optional source file (Native).
    fn cluster_row(
        &self,
        ix: usize,
        info: &ClusterInfo,
        style: &NavStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.active.as_ref() == Some(&info.id);
        let target = info.id.clone();
        let hue = avatar_hue(&info.name);
        let tip = SharedString::from(cluster_tooltip(info));
        let (selected_bg, selected_fg, text, muted, hover) = (
            style.selected_bg,
            style.selected_fg,
            style.text,
            style.muted,
            style.hover,
        );
        let badge = if style.squares {
            div()
                .flex_none()
                .size(px(16.))
                .rounded(px(4.))
                .bg(hsla(hue, 0.65, 0.5, 1.))
        } else {
            div()
                .flex_none()
                .size(px(18.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(hue, 0.35, 0.22, 1.))
                .text_color(hsla(hue, 0.6, 0.82, 1.))
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(info.initials())
        };
        h_flex()
            .id(("nav-cluster", ix))
            .w_full()
            .px_2()
            .py_1p5()
            .gap_2()
            .items_center()
            .rounded(style.radius)
            .cursor_pointer()
            .text_sm()
            .text_color(if active { selected_fg } else { text })
            .when(active, |this| this.bg(selected_bg))
            .hover(move |s| s.bg(if active { selected_bg } else { hover }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.dispatch(AppCommand::SelectCluster(target.clone()), window, cx);
            }))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .child(badge)
            .child(div().flex_1().truncate().child(info.name.clone()))
            .when(style.squares, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(if active { selected_fg } else { muted })
                        .child(info.source_file_name().unwrap_or_else(|| "default".into())),
                )
            })
            .into_any_element()
    }

    /// A plain "+ Add kubeconfig…" style row.
    fn action_row(
        &self,
        id: &'static str,
        label: &'static str,
        command: AppCommand,
        style: &NavStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = style.hover;
        h_flex()
            .id(id)
            .w_full()
            .px_2()
            .py_1p5()
            .gap_2()
            .items_center()
            .rounded(style.radius)
            .cursor_pointer()
            .text_sm()
            .text_color(style.muted)
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.dispatch(command.clone(), window, cx);
            }))
            .child(div().w(px(16.)).flex().justify_center().child("+"))
            .child(label)
            .into_any_element()
    }

    fn link_row(
        &self,
        ix: usize,
        link: &NavLink,
        style: &NavStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.nav.view();
        let dynamic_current = self
            .nav
            .section_index()
            .and_then(|ix| self.nav.sections().get(ix))
            .is_some_and(|s| s.dynamic);
        let active = current == &link.view || (link.label == "Custom Resources" && dynamic_current);
        let count = (active && matches!(link.view, ViewId::Resource(_)) && !dynamic_current)
            .then(|| self.active_count(cx))
            .filter(|n| *n > 0);
        let view = link.view.clone();
        let (selected_bg, selected_fg, text, muted, hover) = (
            style.selected_bg,
            style.selected_fg,
            style.text,
            style.muted,
            style.hover,
        );
        h_flex()
            .id(("nav-link", ix))
            .w_full()
            .px_2()
            .py_1p5()
            .gap_2()
            .items_center()
            .rounded(style.radius)
            .cursor_pointer()
            .text_sm()
            .text_color(if active { selected_fg } else { text })
            .when(active, |this| this.bg(selected_bg))
            .hover(move |s| s.bg(if active { selected_bg } else { hover }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.dispatch(AppCommand::Navigate(view.clone()), window, cx);
            }))
            .child(div().flex_1().truncate().child(link.label.clone()))
            .when_some(count, |this, n| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(if active { selected_fg } else { muted })
                        .child(n.to_string()),
                )
            })
            .into_any_element()
    }

    fn caption(&self, text: &str, style: &NavStyle) -> Div {
        div()
            .px_2()
            .mb_1()
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(style.muted)
            .child(if style.upper_captions {
                text.to_uppercase()
            } else {
                text.to_string()
            })
    }

    /// Clusters block plus the captioned navigation groups (Command, Native).
    pub(crate) fn nav_sidebar(&self, style: &NavStyle, cx: &mut Context<Self>) -> AnyElement {
        let mut cluster_rows: Vec<AnyElement> = self
            .clusters
            .iter()
            .enumerate()
            .map(|(ix, info)| self.cluster_row(ix, info, style, cx))
            .collect();
        cluster_rows.push(self.action_row(
            "nav-add-kubeconfig",
            "Add kubeconfig…",
            AppCommand::AddKubeconfig,
            style,
            cx,
        ));
        cluster_rows.push(self.action_row(
            "nav-import-freelens",
            "Import from Freelens…",
            AppCommand::ImportFreelens,
            style,
            cx,
        ));

        let mut blocks: Vec<AnyElement> = vec![
            v_flex()
                .gap_px()
                .child(self.caption("Clusters", style))
                .children(cluster_rows)
                .into_any_element(),
        ];
        let mut ix = 0;
        for group in sidebar_groups(self.nav.sections()) {
            let rows: Vec<AnyElement> = group
                .links
                .iter()
                .map(|link| {
                    ix += 1;
                    self.link_row(ix, link, style, cx)
                })
                .collect();
            blocks.push(
                v_flex()
                    .gap_px()
                    .child(self.caption(&group.caption, style))
                    .children(rows)
                    .into_any_element(),
            );
        }

        v_flex()
            .id("nav-sidebar")
            .h_full()
            .w(px(style.width))
            .flex_none()
            .overflow_y_scroll()
            .px_2p5()
            .py_4()
            .gap_5()
            .bg(style.background)
            .border_r_1()
            .border_color(style.border)
            .children(blocks)
            .into_any_element()
    }

    /// Fixed right column hosting the detail panel (Native inspector).
    pub(crate) fn render_inspector(
        &self,
        detail: Option<Entity<natikube_detail::DetailPanel>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        // Nothing selected: no inspector, so the table keeps the full width.
        let Some(detail) = detail.filter(|_| self.detail_open) else {
            return div().into_any_element();
        };
        let body = div().size_full().child(detail).into_any_element();
        div()
            .id("inspector")
            .h_full()
            .w(px(360. * cx.tokens().scale))
            .flex_none()
            .bg(theme.sidebar)
            .border_l_1()
            .border_color(theme.border)
            .child(body)
            .into_any_element()
    }
}

/// The shortcut chip shown in the search box: `secondary` is Cmd on macOS and
/// Ctrl elsewhere, so the hint must match the key that really opens the palette.
fn palette_hint(is_macos: bool) -> &'static str {
    if is_macos { "⌘K" } else { "Ctrl+K" }
}

#[cfg(test)]
mod palette_hint_tests {
    use super::palette_hint;

    #[test]
    fn hint_matches_the_platform_modifier() {
        assert_eq!(palette_hint(true), "⌘K");
        assert_eq!(palette_hint(false), "Ctrl+K");
    }
}
