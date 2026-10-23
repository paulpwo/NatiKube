use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::*;
use natikube_core::{AppCommand, ClusterId, ClusterInfo, ConnectionState};
use natikube_ui_kit::{ActiveTokens as _, Tone, status_dot};

/// The cluster header at the top of the sidebar: name, connection state dot
/// and a dropdown listing every known context. Picking another cluster emits
/// [`AppCommand::SelectCluster`].
pub struct ClusterHeader {
    cluster: Option<ClusterInfo>,
    state: ConnectionState,
    clusters: Vec<ClusterInfo>,
}

impl EventEmitter<AppCommand> for ClusterHeader {}

impl ClusterHeader {
    pub fn new(cluster: Option<ClusterInfo>, state: ConnectionState) -> Self {
        Self {
            cluster,
            state,
            clusters: Vec::new(),
        }
    }

    /// The contexts offered by the dropdown.
    pub fn set_clusters(&mut self, clusters: Vec<ClusterInfo>, cx: &mut Context<Self>) {
        self.clusters = clusters;
        cx.notify();
    }

    pub fn set_cluster(&mut self, cluster: Option<ClusterInfo>, cx: &mut Context<Self>) {
        self.cluster = cluster;
        cx.notify();
    }

    pub fn set_state(&mut self, state: ConnectionState, cx: &mut Context<Self>) {
        self.state = state;
        cx.notify();
    }
}

impl Render for ClusterHeader {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.tokens().palette;
        let name: SharedString = self
            .cluster
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "No cluster".to_string())
            .into();
        let tone = match &self.state {
            ConnectionState::Connected => Tone::Ok,
            ConnectionState::Connecting => Tone::Warning,
            ConnectionState::Failed(_) => Tone::Error,
            ConnectionState::Disconnected => Tone::Muted,
        };
        let active: Option<ClusterId> = self.cluster.as_ref().map(|c| c.id.clone());
        let entries: Vec<(ClusterId, SharedString)> = self
            .clusters
            .iter()
            .map(|c| {
                // Source file next to the name, so same-named contexts and
                // backups are told apart.
                let label = match c.source_file_name() {
                    Some(file) => format!("{}  \u{b7}  {file}", c.name),
                    None => c.name.clone(),
                };
                (c.id.clone(), SharedString::from(label))
            })
            .collect();
        let this = cx.entity();
        let state_text: SharedString = self.state.label().into();

        let trigger = Button::new("cluster-dropdown")
            .ghost()
            .w_full()
            .label(format!("{name}  \u{25be}"))
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu;
                if entries.is_empty() {
                    menu = menu.item(PopupMenuItem::label("No clusters found"));
                }
                for (id, label) in entries.clone() {
                    let this = this.clone();
                    let checked = active.as_ref() == Some(&id);
                    menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                        move |_, _, cx| {
                            let id = id.clone();
                            this.update(cx, |_, cx| cx.emit(AppCommand::SelectCluster(id)));
                        },
                    ));
                }
                menu = menu.item(PopupMenuItem::separator());
                let add = this.clone();
                menu = menu.item(PopupMenuItem::new("Add kubeconfig\u{2026}").on_click(
                    move |_, _, cx| {
                        add.update(cx, |_, cx| cx.emit(AppCommand::AddKubeconfig));
                    },
                ));
                let import = this.clone();
                menu.item(PopupMenuItem::new("Import from Freelens\u{2026}").on_click(
                    move |_, _, cx| {
                        import.update(cx, |_, cx| cx.emit(AppCommand::ImportFreelens));
                    },
                ))
            });

        h_flex()
            .w_full()
            .h_12()
            .flex_none()
            .px_3()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .id("cluster-state")
                    .child(status_dot(tone, cx))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(state_text.clone())
                            .build(window, cx)
                    }),
            )
            .child(div().flex_1().overflow_hidden().child(trigger))
    }
}
