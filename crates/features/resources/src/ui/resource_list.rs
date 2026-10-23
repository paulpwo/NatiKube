use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use gpui_kit::component::Size;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::table::{DataTable, TableEvent, TableState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::{
    AppCommand, Appearance, ClusterId, NamespaceFilter, ResourceKind, ResourceRef, ViewId,
    age_since, format_cpu, format_memory, kinds,
};
use natikube_ui_kit::{ActiveTokens as _, Palette, Tone, error_state};

use crate::domain::{
    ColumnKind, GroupItem, ObjectSource, ObjectTable, Printer, RowEvent, StatusFilter, UsageSource,
    cluster_kpis, count_by_health, flatten_groups, group_by_namespace, printer_for,
};
use crate::ui::feed::{pump_rows, pump_usage, tick_ages, unix_now};
use crate::ui::objects_table::{ObjectsDelegate, ref_at, tone};

/// The table of one resource kind: header (title, item count, namespace
/// filter, search), sortable table, inline errors. One watcher (plus one
/// metrics poller for pods and nodes) lives exactly as long as this view.
pub struct ResourceList {
    cluster: ClusterId,
    kind: ResourceKind,
    title: SharedString,
    table: Entity<TableState<ObjectsDelegate>>,
    search: Entity<InputState>,
    selected: BTreeSet<String>,
    known_namespaces: Vec<String>,
    error: Option<SharedString>,
    loaded: bool,
    /// Observatory: namespaces whose section is folded.
    collapsed: BTreeSet<String>,
    scroll: UniformListScrollHandle,
    /// Observatory pod lists: nodes, for the capacity KPI cards. The watcher
    /// starts the first time that layout is drawn.
    nodes: ObjectTable,
    nodes_source: Option<Arc<dyn ObjectSource>>,
    nodes_task: Option<Task<()>>,
    /// Metrics poller, started once the first listing arrived so it does not
    /// compete with it for a connection.
    usage_source: Option<Arc<dyn UsageSource>>,
    /// Width of the table area, measured while painting; the columns are
    /// fitted to it (see `fit_widths`).
    body_width: std::rc::Rc<std::cell::Cell<f32>>,
    _tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// Horizontal room kept for the table's vertical scrollbar.
const SCROLLBAR_ROOM: f32 = 12.0;

/// Most loaded rows offered to the command palette.
const LOADED_REFS_CAP: usize = 2000;

impl EventEmitter<AppCommand> for ResourceList {}

impl ResourceList {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cluster: ClusterId,
        kind: ResourceKind,
        printer: Printer,
        source: Arc<dyn ObjectSource>,
        usage: Option<Arc<dyn UsageSource>>,
        namespaces: NamespaceFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title: SharedString = ViewId::Resource(kind.clone()).title().into();
        let mut object_table = ObjectTable::new(printer.columns().to_vec());
        object_table.set_namespaces(namespaces.clone());
        let owner = cx.weak_entity();
        let delegate = ObjectsDelegate::new(
            object_table,
            cluster.clone(),
            kind.clone(),
            owner,
            unix_now(),
        );
        let table = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .sortable(true)
                .col_movable(false)
        });
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(format!("Search {title}...")));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(
            &table,
            |this: &mut Self, table, event: &TableEvent, cx| {
                if let TableEvent::SelectRow(ix) = event
                    && let Some(target) = ref_at(table.read(cx).delegate(), *ix)
                {
                    let _ = this;
                    cx.emit(AppCommand::OpenDetail(target));
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = state.read(cx).value();
                    this.table.update(cx, |state, cx| {
                        state.delegate_mut().table.set_search(&text);
                        cx.notify();
                    });
                    cx.notify();
                }
            },
        ));

        let tasks = vec![
            pump_rows(source.as_ref(), cx, |this: &mut Self, batch, cx| {
                this.apply_batch(batch, cx)
            }),
            tick_ages(
                cx,
                |this: &Self, cx| this.table.read(cx).delegate().rendered_newest(),
                |this: &mut Self, cx| {
                    this.table.update(cx, |state, cx| {
                        let delegate = state.delegate_mut();
                        delegate.take_rendered_newest();
                        delegate.now = unix_now();
                        cx.notify();
                    });
                },
            ),
        ];

        let selected = match &namespaces {
            NamespaceFilter::All => BTreeSet::new(),
            NamespaceFilter::Only(set) => set.clone(),
        };
        Self {
            cluster,
            kind,
            title,
            table,
            search,
            selected,
            known_namespaces: Vec::new(),
            error: None,
            loaded: false,
            collapsed: BTreeSet::new(),
            scroll: UniformListScrollHandle::new(),
            nodes: ObjectTable::new(printer_for(&kinds::NODE).columns().to_vec()),
            nodes_source: None,
            nodes_task: None,
            usage_source: usage,
            body_width: Default::default(),
            _tasks: tasks,
            _subscriptions: subscriptions,
        }
    }

    /// Whether the first listing has arrived.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The watcher's current failure text, if any.
    pub fn error(&self) -> Option<&SharedString> {
        self.error.as_ref()
    }

    fn start_usage(&mut self, cx: &mut Context<Self>) {
        let Some(usage) = self.usage_source.take() else {
            return;
        };
        let poller = pump_usage(usage.as_ref(), cx, |this: &mut Self, usage, cx| {
            this.table.update(cx, |state, cx| {
                if state.delegate_mut().table.set_usage(usage) {
                    cx.notify();
                }
            });
        });
        self._tasks.push(poller);
    }

    /// Where the node list for the capacity KPIs comes from (pod lists only).
    /// Nothing is watched until the Observatory layout is shown.
    pub fn set_nodes_source(&mut self, source: Arc<dyn ObjectSource>) {
        self.nodes_source = Some(source);
        self.nodes_task = None;
    }

    pub fn kind(&self) -> &ResourceKind {
        &self.kind
    }

    pub fn cluster(&self) -> &ClusterId {
        &self.cluster
    }

    /// Rows currently shown (after search and namespace filter).
    pub fn item_count(&self, cx: &App) -> usize {
        self.table.read(cx).delegate().table.len()
    }

    /// References to the rows currently loaded (namespace filter applied),
    /// for the shell's command palette. Capped so a huge list stays cheap.
    pub fn loaded_refs(&self, cx: &App) -> Vec<ResourceRef> {
        self.table
            .read(cx)
            .delegate()
            .table
            .namespace_filtered()
            .take(LOADED_REFS_CAP)
            .map(|row| {
                ResourceRef::new(
                    self.cluster.clone(),
                    self.kind.clone(),
                    row.namespace.as_deref().map(str::to_string),
                    row.name.clone(),
                )
            })
            .collect()
    }

    pub(crate) fn emit_command(&mut self, command: AppCommand, cx: &mut Context<Self>) {
        cx.emit(command);
    }

    /// All namespaces of the cluster, for the filter dropdown.
    pub fn set_known_namespaces(&mut self, names: Vec<String>, cx: &mut Context<Self>) {
        if self.known_namespaces != names {
            self.known_namespaces = names;
            cx.notify();
        }
    }

    /// Applies a filter chosen elsewhere (the shell restoring preferences).
    pub fn set_namespaces(&mut self, filter: NamespaceFilter, cx: &mut Context<Self>) {
        self.selected = match &filter {
            NamespaceFilter::All => BTreeSet::new(),
            NamespaceFilter::Only(set) => set.clone(),
        };
        self.push_filter(cx);
    }

    fn filter(&self) -> NamespaceFilter {
        if self.selected.is_empty() {
            NamespaceFilter::All
        } else {
            NamespaceFilter::Only(self.selected.clone())
        }
    }

    fn push_filter(&mut self, cx: &mut Context<Self>) {
        let filter = self.filter();
        self.table.update(cx, |state, cx| {
            state.delegate_mut().table.set_namespaces(filter);
            cx.notify();
        });
        cx.notify();
    }

    fn toggle_namespace(&mut self, name: &str, on: bool, cx: &mut Context<Self>) {
        if on {
            self.selected.insert(name.to_string());
        } else {
            self.selected.remove(name);
        }
        self.push_filter(cx);
        cx.emit(AppCommand::NamespaceFilterChanged(self.filter()));
    }

    fn clear_namespaces(&mut self, cx: &mut Context<Self>) {
        self.selected.clear();
        self.push_filter(cx);
        cx.emit(AppCommand::NamespaceFilterChanged(NamespaceFilter::All));
    }

    /// Known namespaces plus any seen in rows or selected.
    fn namespace_options(&self, cx: &App) -> Vec<String> {
        let mut names: BTreeSet<String> = self.known_namespaces.iter().cloned().collect();
        names.extend(self.selected.iter().cloned());
        names.extend(
            self.table
                .read(cx)
                .delegate()
                .table
                .all_rows()
                .iter()
                .filter_map(|r| r.namespace.as_deref().map(str::to_string)),
        );
        names.into_iter().collect()
    }

    fn apply_batch(&mut self, batch: Vec<crate::ui::feed::RowItem>, cx: &mut Context<Self>) {
        let mut error = self.error.clone();
        let mut loaded = self.loaded;
        let title = self.title.to_lowercase();
        let rows_changed = self.table.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            let mut changed = false;
            for item in batch {
                match item {
                    Ok(event) => {
                        error = None;
                        if matches!(event, RowEvent::Restarted(_)) {
                            loaded = true;
                            delegate.empty_message = format!("No {title} found").into();
                        }
                        changed |= delegate.table.apply(event);
                    }
                    Err(e) => error = Some(e.to_string().into()),
                }
            }
            if changed {
                delegate.now = unix_now();
                cx.notify();
            }
            changed
        });
        let first_load = loaded && !self.loaded;
        if first_load {
            self.start_usage(cx);
        }
        if rows_changed || self.error != error || self.loaded != loaded {
            self.error = error;
            self.loaded = loaded;
            cx.notify();
        }
    }

    fn namespace_dropdown(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label: SharedString = self.filter().label().into();
        let me = cx.weak_entity();
        Popover::new("namespace-filter")
            .anchor(Anchor::TopRight)
            .trigger(
                Button::new("namespace-trigger")
                    .outline()
                    .small()
                    .dropdown_caret(true)
                    .label(label),
            )
            .content(move |_, _, cx| {
                let options = me
                    .read_with(cx, |list, cx| {
                        (list.namespace_options(cx), list.selected.clone())
                    })
                    .unwrap_or_default();
                let (names, selected) = options;
                v_flex()
                    .id("namespace-options")
                    .w(px(240.))
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .gap_1()
                    .child({
                        let me = me.clone();
                        Checkbox::new("ns-all")
                            .label("All namespaces")
                            .checked(selected.is_empty())
                            .on_click(move |_, _, cx| {
                                me.update(cx, |list, cx| list.clear_namespaces(cx)).ok();
                            })
                    })
                    .children(names.into_iter().enumerate().map(|(ix, name)| {
                        let me = me.clone();
                        let checked = selected.contains(&name);
                        let toggled = name.clone();
                        Checkbox::new(("ns", ix))
                            .label(name)
                            .checked(checked)
                            .on_click(move |on, _, cx| {
                                let on = *on;
                                me.update(cx, |list, cx| list.toggle_namespace(&toggled, on, cx))
                                    .ok();
                            })
                    }))
            })
    }
}

/// Height of one line of the grouped (Observatory) list.
const GROUP_LINE: f32 = 38.;

/// Kinds offered by the Observatory chip row, in the canvas order.
const CHIP_KINDS: [ResourceKind; 5] = [
    kinds::POD,
    kinds::DEPLOYMENT,
    kinds::STATEFUL_SET,
    kinds::DAEMON_SET,
    kinds::JOB,
];

/// Palette and font values the appearance-specific layouts paint with.
#[derive(Clone)]
struct Skin {
    appearance: Appearance,
    palette: Palette,
    ui: SharedString,
    mono: SharedString,
    radius_md: Pixels,
    radius_lg: Pixels,
}

impl Skin {
    fn read(cx: &App) -> Self {
        let tokens = cx.tokens();
        Self {
            appearance: tokens.appearance,
            palette: tokens.palette,
            ui: tokens.fonts.ui.clone(),
            mono: tokens.fonts.mono.clone(),
            radius_md: tokens.radii.md,
            radius_lg: tokens.radii.lg,
        }
    }
}

impl ResourceList {
    /// Lays the table out for the active appearance and starts the node feed
    /// the Observatory KPI cards need.
    fn sync_appearance(&mut self, cx: &mut Context<Self>) {
        let tokens = cx.tokens().clone();
        let width = self.body_width.get();
        if width > 0.0 {
            // Leave room for the vertical scrollbar.
            let available = width - SCROLLBAR_ROOM;
            self.table.update(cx, |state, cx| {
                if state.delegate_mut().fit_to(available, &tokens) {
                    state.refresh(cx);
                    cx.notify();
                }
            });
        }
        let laid_out = self.table.read(cx).delegate();
        if laid_out.look() != tokens.appearance || laid_out.scale() != tokens.scale {
            self.table.update(cx, |state, cx| {
                let delegate = state.delegate_mut();
                let changed = delegate.apply_look(&tokens);
                // Only Command and Native expose the status filter.
                if !matches!(tokens.appearance, Appearance::Command | Appearance::Native) {
                    delegate.table.set_status_filter(StatusFilter::All);
                }
                if changed {
                    state.refresh(cx);
                }
                cx.notify();
            });
        }
        if tokens.group_by_namespace
            && self.nodes_task.is_none()
            && let Some(source) = self.nodes_source.clone()
        {
            self.nodes_task = Some(pump_rows(
                source.as_ref(),
                cx,
                |this: &mut Self, batch, cx| {
                    for event in batch.into_iter().flatten() {
                        this.nodes.apply(event);
                    }
                    cx.notify();
                },
            ));
        }
    }

    fn set_status_filter(&mut self, filter: StatusFilter, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().table.set_status_filter(filter);
            cx.notify();
        });
        cx.notify();
    }

    fn toggle_group(&mut self, namespace: &str, cx: &mut Context<Self>) {
        if !self.collapsed.remove(namespace) {
            self.collapsed.insert(namespace.to_string());
        }
        cx.notify();
    }

    fn error_note(&self, show_error: bool, cx: &App) -> Option<Div> {
        let error = self.error.clone().filter(|_| !show_error)?;
        Some(
            div()
                .text_xs()
                .text_color(Tone::Warning.color(cx))
                .child(error),
        )
    }

    fn search_box(&self, width: f32) -> Div {
        div()
            .w(px(width))
            .child(Input::new(&self.search).small().cleanable(true))
    }

    /// Today's header, untouched.
    fn standard_header(&self, items: String, show_error: bool, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let namespaced = self.kind.namespaced;
        h_flex()
            .w_full()
            .h_12()
            .flex_none()
            .px_4()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(border)
            .child(div().text_base().child(self.title.clone()))
            .child(div().text_color(muted).child(items))
            .children(self.error_note(show_error, cx))
            .child(div().flex_1())
            .when(namespaced, |this| this.child(self.namespace_dropdown(cx)))
            .child(self.search_box(240.))
    }

    /// Command: title with a health summary, pill filters.
    fn command_header(&self, skin: &Skin, show_error: bool, cx: &mut Context<Self>) -> Div {
        let p = skin.palette;
        let (counts, filter) = {
            let table = &self.table.read(cx).delegate().table;
            (
                count_by_health(table.namespace_filtered()),
                table.status_filter(),
            )
        };
        let summary = if self.kind == kinds::POD {
            format!(
                "{} running · {} pending · {} failing",
                counts.ok, counts.warning, counts.error
            )
        } else {
            format!(
                "{} healthy · {} warning · {} failing",
                counts.ok, counts.warning, counts.error
            )
        };
        let status_label = match filter {
            StatusFilter::All => "Status: any".to_string(),
            other => format!("Status: {}", other.label().to_lowercase()),
        };
        let me = cx.weak_entity();
        let status = Button::new("status-filter")
            .outline()
            .small()
            .rounded(px(15.))
            .label(status_label)
            .dropdown_menu(move |mut menu, _, _| {
                for option in StatusFilter::all() {
                    let me = me.clone();
                    menu = menu.item(PopupMenuItem::new(option.label()).on_click(
                        move |_, _, cx| {
                            me.update(cx, |list, cx| list.set_status_filter(option, cx))
                                .ok();
                        },
                    ));
                }
                menu
            });
        h_flex()
            .w_full()
            .flex_none()
            .px_6()
            .pt_5()
            .pb_3()
            .gap_3()
            .items_center()
            .flex_wrap()
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(div().text_xs().text_color(p.text_muted).child(summary))
            .children(self.error_note(show_error, cx))
            .child(div().flex_1())
            .when(self.kind.namespaced, |this| {
                this.child(self.namespace_dropdown(cx))
            })
            .child(status)
            .child(self.search_box(220.))
    }

    /// Native: segmented All / Running / Problems control. The shell toolbar
    /// owns the title and subtitle.
    fn native_header(&self, skin: &Skin, show_error: bool, cx: &mut Context<Self>) -> Div {
        let p = skin.palette;
        let (counts, current) = {
            let table = &self.table.read(cx).delegate().table;
            (table.status_filter_counts(), table.status_filter())
        };
        let segments = StatusFilter::all().into_iter().map(|option| {
            let active = option == current;
            div()
                .id(("status-segment", option as usize))
                .h(px(24.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(p.text)
                .when(active, |this| this.bg(p.background).shadow_sm())
                .on_click(cx.listener(move |list, _, _, cx| list.set_status_filter(option, cx)))
                .child(format!("{} {}", option.label(), counts.get(option)))
        });
        h_flex()
            .w_full()
            .flex_none()
            .px_5()
            .py_3()
            .gap_4()
            .items_center()
            .flex_wrap()
            .border_b_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .p(px(2.))
                    .gap(px(1.))
                    .rounded(px(7.))
                    .bg(p.panel_raised)
                    .children(segments),
            )
            .children(self.error_note(show_error, cx))
            .child(div().flex_1())
            .when(self.kind.namespaced, |this| {
                this.child(self.namespace_dropdown(cx))
            })
            .child(self.search_box(220.))
    }

    /// Observatory: cluster heading, KPI cards (pods), kind chips.
    fn observatory_header(
        &self,
        skin: &Skin,
        items: String,
        show_error: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = skin.palette;
        let heading = h_flex()
            .items_baseline()
            .gap_3()
            .flex_wrap()
            .child(
                div()
                    .text_size(px(22.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.cluster.context_name().to_string()),
            )
            .child(
                div()
                    .text_color(p.text_muted)
                    .child(format!("{} · grouped by namespace", self.title)),
            )
            .child(div().text_xs().text_color(p.text_muted).child(items))
            .children(self.error_note(show_error, cx))
            .child(div().flex_1())
            .child(self.namespace_dropdown(cx))
            .child(self.search_box(260.));

        let chips =
            h_flex()
                .gap_2()
                .flex_wrap()
                .children(CHIP_KINDS.into_iter().enumerate().map(|(ix, kind)| {
                    let active = kind == self.kind;
                    let label = ViewId::Resource(kind.clone()).title();
                    let label = if active {
                        let count = self.table.read(cx).delegate().table.len();
                        format!("{label} {count}")
                    } else {
                        label
                    };
                    let target = ViewId::Resource(kind);
                    div()
                        .id(("kind-chip", ix))
                        .h(px(30.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .rounded(skin.radius_md)
                        .border_1()
                        .cursor_pointer()
                        .when(active, |this| {
                            this.border_color(p.warning)
                                .bg(p.warning.opacity(0.12))
                                .text_color(p.warning)
                        })
                        .when(!active, |this| {
                            this.border_color(p.border).text_color(p.text)
                        })
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(AppCommand::Navigate(target.clone()))
                        }))
                        .child(label)
                }));

        v_flex()
            .w_full()
            .flex_none()
            .px_6()
            .pt_5()
            .pb_3()
            .gap_4()
            .child(heading)
            .when(self.kind == kinds::POD, |this| {
                this.child(self.kpi_row(skin, cx))
            })
            .child(chips)
    }

    /// Pods, nodes, CPU and memory cards.
    fn kpi_row(&self, skin: &Skin, cx: &App) -> Div {
        let p = skin.palette;
        let table = &self.table.read(cx).delegate().table;
        let kpis = cluster_kpis(table.all_rows(), table.usage(), self.nodes.all_rows());

        let card = |title: &str| {
            v_flex()
                .flex_1()
                .min_w(px(180.))
                .gap(px(6.))
                .px(px(16.))
                .py(px(14.))
                .rounded(skin.radius_lg)
                .bg(p.panel)
                .border_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_xs()
                        .text_color(p.text_muted)
                        .child(title.to_string()),
                )
        };
        let big = |text: String, mono: bool| {
            div()
                .text_size(px(24.))
                .font_weight(FontWeight::SEMIBOLD)
                .when(mono, |this| this.font_family(skin.mono.clone()))
                .child(text)
        };
        let note =
            |text: String, color: Hsla| div().text_size(px(13.)).text_color(color).child(text);
        let gauge = |ratio: Option<f32>| {
            div()
                .w_full()
                .h(px(6.))
                .rounded(px(3.))
                .bg(p.border)
                .when_some(ratio, |this, ratio| {
                    this.child(
                        div()
                            .h(px(6.))
                            .w(relative(ratio))
                            .rounded(px(3.))
                            .bg(p.warning),
                    )
                })
        };

        let pods_note = if kpis.pods_total > 0 && kpis.pods_running == kpis.pods_total {
            note("all running".into(), p.ok)
        } else if kpis.pods_problems > 0 {
            note(
                format!(
                    "{} running · {} problems",
                    kpis.pods_running, kpis.pods_problems
                ),
                p.warning,
            )
        } else {
            note(format!("{} running", kpis.pods_running), p.text_muted)
        };
        let nodes_note = if kpis.nodes_ready == kpis.nodes_total {
            note("ready".into(), p.ok)
        } else {
            note(format!("ready of {}", kpis.nodes_total), p.warning)
        };
        let metrics_missing = || note("metrics unavailable".into(), p.text_muted);

        let cpu = match kpis.cpu_used {
            Some(used) => card("CPU used by pods")
                .child(
                    h_flex()
                        .items_baseline()
                        .gap_2()
                        .child(big(format!("{used:.3}"), true))
                        .child(note(
                            match kpis.cpu_capacity {
                                Some(cap) => format!("cores of {}", format_cpu(cap)),
                                None => "cores".into(),
                            },
                            p.text_muted,
                        )),
                )
                .child(gauge(kpis.cpu_ratio())),
            None => card("CPU used by pods")
                .child(big("—".into(), false))
                .child(metrics_missing()),
        };
        let memory = match kpis.memory_used {
            Some(used) => card("Memory used by pods")
                .child(
                    h_flex()
                        .items_baseline()
                        .gap_2()
                        .child(big(format_memory(used), true))
                        .when_some(kpis.memory_capacity, |this, cap| {
                            this.child(note(format!("of {}", format_memory(cap)), p.text_muted))
                        }),
                )
                .child(gauge(kpis.memory_ratio())),
            None => card("Memory used by pods")
                .child(big("—".into(), false))
                .child(metrics_missing()),
        };

        h_flex()
            .w_full()
            .gap_3()
            .flex_wrap()
            .child(
                card("Pods").child(
                    h_flex()
                        .items_baseline()
                        .gap_2()
                        .child(big(kpis.pods_total.to_string(), false))
                        .child(pods_note),
                ),
            )
            .child(
                card("Nodes").child(
                    h_flex()
                        .items_baseline()
                        .gap_2()
                        .child(big(kpis.nodes_ready.to_string(), false))
                        .child(nodes_note),
                ),
            )
            .child(cpu)
            .child(memory)
    }

    /// Namespace sections (header plus pod lines) in one virtualized list.
    fn grouped_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let lines = {
            let table = &self.table.read(cx).delegate().table;
            flatten_groups(&group_by_namespace(table), &self.collapsed).len()
        };
        if lines == 0 {
            let message = self.table.read(cx).delegate().empty_message.clone();
            return h_flex()
                .size_full()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child(message)
                .into_any_element();
        }
        uniform_list(
            "grouped-resources",
            lines,
            cx.processor(|this, range: Range<usize>, _, cx| this.grouped_lines(range, cx)),
        )
        .track_scroll(&self.scroll)
        .size_full()
        .into_any_element()
    }

    fn grouped_lines(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let skin = Skin::read(cx);
        let p = skin.palette;
        let state = self.table.read(cx);
        let delegate = state.delegate();
        let table = &delegate.table;
        let groups = group_by_namespace(table);
        let lines = flatten_groups(&groups, &self.collapsed);
        let has = |kind: ColumnKind| table.columns().iter().any(|c| c.kind == kind);
        let (has_cpu, has_mem, has_owner, has_age) = (
            has(ColumnKind::Cpu),
            has(ColumnKind::Memory),
            has(ColumnKind::Owner),
            has(ColumnKind::Age),
        );
        let noun = self.title.to_lowercase();
        let height = px(GROUP_LINE);

        lines
            .iter()
            .enumerate()
            .skip(range.start)
            .take(range.len())
            .filter_map(|(line_ix, line)| match *line {
                GroupItem::Header(group_ix) => {
                    let group = &groups[group_ix];
                    let namespace = group.namespace.clone();
                    let folded = self.collapsed.contains(&namespace);
                    let totals = if group.has_usage {
                        format!(
                            "{} cores · {}",
                            format_cpu(group.cpu_cores),
                            format_memory(group.memory_bytes)
                        )
                    } else {
                        String::new()
                    };
                    let toggled = namespace.clone();
                    Some(
                        h_flex()
                            .id(("group-header", line_ix))
                            .w_full()
                            .h(height)
                            .px_4()
                            .gap_3()
                            .items_center()
                            .bg(p.panel_raised)
                            .border_t_1()
                            .border_color(p.border)
                            .cursor_pointer()
                            .on_click(
                                cx.listener(move |list, _, _, cx| list.toggle_group(&toggled, cx)),
                            )
                            .child(
                                div()
                                    .w(px(12.))
                                    .text_xs()
                                    .text_color(p.text_muted)
                                    .child(if folded { "▸" } else { "▾" }),
                            )
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(
                                if namespace.is_empty() {
                                    "(cluster)".to_string()
                                } else {
                                    namespace
                                },
                            ))
                            .child(
                                div()
                                    .text_color(p.text_muted)
                                    .child(format!("{} {noun}", group.rows.len())),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .font_family(skin.mono.clone())
                                    .text_size(px(12.))
                                    .text_color(p.text_muted)
                                    .child(totals),
                            )
                            .into_any_element(),
                    )
                }
                GroupItem::Row(row_ix) => {
                    let row = table.row(row_ix)?;
                    let usage = table
                        .usage()
                        .for_object(row.namespace.as_deref(), &row.name);
                    let cpu = usage.map(|u| format_cpu(u.cpu_cores)).unwrap_or_default();
                    let mem = usage
                        .map(|u| format_memory(u.memory_bytes))
                        .unwrap_or_default();
                    let owner = row
                        .owner
                        .as_ref()
                        .map(|o| o.kind.to_string())
                        .unwrap_or_default();
                    delegate.note_age(row.created_at);
                    let age = age_since(row.created_at, delegate.now);
                    let target = ref_at(delegate, row_ix);
                    let cell = |text: String, width: f32, mono: bool| {
                        div()
                            .w(px(width))
                            .flex_none()
                            .truncate()
                            .text_size(px(12.))
                            .when(mono, |this| this.font_family(skin.mono.clone()))
                            .text_color(if mono { p.text } else { p.text_muted })
                            .child(text)
                    };
                    Some(
                        h_flex()
                            .id(("group-row", line_ix))
                            .w_full()
                            .h(height)
                            .px_4()
                            .gap_3()
                            .items_center()
                            .border_t_1()
                            .border_color(p.border)
                            .cursor_pointer()
                            .hover(|s| s.bg(p.panel_raised))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                if let Some(target) = target.clone() {
                                    cx.emit(AppCommand::OpenDetail(target));
                                }
                            }))
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(8.))
                                    .rounded_full()
                                    .bg(tone(row.health).color(cx)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(skin.mono.clone())
                                    .text_size(px(12.5))
                                    .child(row.name.clone()),
                            )
                            .when(has_cpu, |this| this.child(cell(cpu, 80., true)))
                            .when(has_mem, |this| this.child(cell(mem, 90., true)))
                            .when(has_owner, |this| this.child(cell(owner, 110., false)))
                            .when(has_age, |this| {
                                this.child(cell(age, 56., false).text_right())
                            })
                            .child(delegate.row_menu(row_ix, row))
                            .into_any_element(),
                    )
                }
            })
            .collect()
    }
}

impl Render for ResourceList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_appearance(cx);
        let skin = Skin::read(cx);
        let (count, total) = {
            let state = self.table.read(cx);
            let table = &state.delegate().table;
            (table.len(), table.total())
        };
        let theme = cx.theme();
        let (background, foreground) = (theme.background, theme.foreground);
        let items = if count == total {
            format!("{count} items")
        } else {
            format!("{count} of {total} items")
        };
        let show_error = self.error.is_some() && count == 0;
        let grouped = skin.appearance == Appearance::Observatory && self.kind.namespaced;

        let header = match skin.appearance {
            Appearance::Standard => self.standard_header(items, show_error, cx),
            Appearance::Command => self.command_header(&skin, show_error, cx),
            Appearance::Native => self.native_header(&skin, show_error, cx),
            Appearance::Observatory if grouped => {
                self.observatory_header(&skin, items, show_error, cx)
            }
            Appearance::Observatory => self.standard_header(items, show_error, cx),
        };

        let body = match &self.error {
            Some(error) if show_error => error_state(
                format!("Cannot list {}", self.title.to_lowercase()),
                error.clone(),
                cx,
            )
            .into_any_element(),
            _ if grouped => self.grouped_list(cx),
            _ => {
                let tokens = cx.tokens();
                let mut table = DataTable::new(&self.table).bordered(false);
                if tokens.appearance != Appearance::Standard {
                    // Stacked names need room for two lines.
                    let row = if tokens.stacked_names {
                        tokens.row_height.max(px(46.))
                    } else {
                        tokens.row_height
                    };
                    table = table
                        .with_size(Size::Size(row))
                        .header_row_height(tokens.header_height);
                }
                let width = self.body_width.clone();
                let me = cx.weak_entity();
                let measure = canvas(
                    move |bounds, _, cx| {
                        let measured: f32 = bounds.size.width.into();
                        if (measured - width.get()).abs() > 1.0 {
                            width.set(measured);
                            me.update(cx, |_, cx| cx.notify()).ok();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full();
                div()
                    .relative()
                    .size_full()
                    .child(table)
                    .child(measure)
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .text_sm()
            .when(skin.appearance != Appearance::Standard, |this| {
                this.font_family(skin.ui.clone())
            })
            .child(header)
            .child(div().flex_1().min_h_0().child(body))
    }
}
