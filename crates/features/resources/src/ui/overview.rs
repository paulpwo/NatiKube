use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::{
    AppCommand, NamespaceFilter, ResourceKind, ViewId, age_since, format_cpu, format_memory, kinds,
};
use natikube_ui_kit::{Tone, section_header};

use crate::domain::{
    ClusterSummary, ObjectTable, StatusCounts, UsageMap, count_by_health, printer_for,
    summarize_cluster,
};
use crate::ui::backend::Backend;
use crate::ui::feed::{RowItem, pump_rows, pump_usage, tick_ages, unix_now};

/// Workload kinds counted by the overview, in Freelens order.
const WORKLOAD_KINDS: [ResourceKind; 8] = [
    kinds::POD,
    kinds::DEPLOYMENT,
    kinds::DAEMON_SET,
    kinds::STATEFUL_SET,
    kinds::REPLICA_SET,
    kinds::REPLICATION_CONTROLLER,
    kinds::JOB,
    kinds::CRON_JOB,
];

/// Applies a batch; returns the last error (if any) and whether the table
/// changed, so callers redraw only when something is new.
fn apply(table: &mut ObjectTable, batch: Vec<RowItem>) -> (Option<String>, bool) {
    let mut error = None;
    let mut changed = false;
    for item in batch {
        match item {
            Ok(event) => changed |= table.apply(event),
            Err(e) => error = Some(e.to_string()),
        }
    }
    (error, changed)
}

// ---- workloads --------------------------------------------------------------

/// Counts of every workload kind by status. One watcher per kind, alive only
/// while the overview is shown.
pub struct WorkloadsOverviewView {
    tables: Vec<(ResourceKind, ObjectTable)>,
    namespaces: NamespaceFilter,
    error: Option<SharedString>,
    _tasks: Vec<Task<()>>,
}

impl EventEmitter<AppCommand> for WorkloadsOverviewView {}

impl WorkloadsOverviewView {
    pub(crate) fn new(
        backend: &Backend,
        namespaces: NamespaceFilter,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut tables = Vec::new();
        let mut tasks = Vec::new();
        for (ix, kind) in WORKLOAD_KINDS.iter().enumerate() {
            let printer = printer_for(kind);
            let mut table = ObjectTable::new(printer.columns().to_vec());
            table.set_namespaces(namespaces.clone());
            tables.push((kind.clone(), table));
            let source = backend.source(kind, &printer);
            tasks.push(pump_rows(
                source.as_ref(),
                cx,
                move |this: &mut Self, batch, cx| {
                    if let Some((_, table)) = this.tables.get_mut(ix) {
                        let (error, changed) = apply(table, batch);
                        let error = error.map(Into::into).or(this.error.take());
                        if changed || error != this.error {
                            cx.notify();
                        }
                        this.error = error;
                    }
                },
            ));
        }
        Self {
            tables,
            namespaces,
            error: None,
            _tasks: tasks,
        }
    }

    pub fn set_namespaces(&mut self, filter: NamespaceFilter, cx: &mut Context<Self>) {
        if self.namespaces != filter {
            for (_, table) in &mut self.tables {
                table.set_namespaces(filter.clone());
            }
            self.namespaces = filter;
            cx.notify();
        }
    }

    /// Counts per kind, in display order.
    pub fn counts(&self) -> Vec<(ResourceKind, StatusCounts)> {
        self.tables
            .iter()
            .map(|(kind, table)| (kind.clone(), count_by_health(table.namespace_filtered())))
            .collect()
    }

    fn card(
        &self,
        ix: usize,
        kind: &ResourceKind,
        counts: StatusCounts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (panel, border, muted, foreground) = (
            theme.muted,
            theme.border,
            theme.muted_foreground,
            theme.foreground,
        );
        let total = counts.total();
        let segments = [
            (counts.ok, Tone::Ok),
            (counts.warning, Tone::Warning),
            (counts.error, Tone::Error),
            (counts.done, Tone::Done),
        ];
        let target = ViewId::Resource(kind.clone());
        v_flex()
            .id(("workload-card", ix))
            .w(px(300.))
            .p_4()
            .gap_3()
            .rounded_md()
            .bg(panel)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .hover(|s| s.border_color(Tone::Done.color(cx)))
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(AppCommand::Navigate(target.clone()))))
            .child(
                h_flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .text_base()
                            .text_color(foreground)
                            .child(ViewId::Resource(kind.clone()).title()),
                    )
                    .child(div().text_color(muted).child(total.to_string())),
            )
            .child(
                h_flex()
                    .w_full()
                    .h(px(6.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(border)
                    .when(total > 0, |bar| {
                        bar.children(segments.iter().filter(|(n, _)| *n > 0).map(|(n, tone)| {
                            div()
                                .h_full()
                                .w(relative(*n as f32 / total as f32))
                                .bg(tone.color(cx))
                        }))
                    }),
            )
            .child(
                h_flex().gap_3().flex_wrap().text_xs().children(
                    [
                        (counts.ok, Tone::Ok, "Running"),
                        (counts.warning, Tone::Warning, "Pending"),
                        (counts.error, Tone::Error, "Failed"),
                        (counts.done, Tone::Done, "Done"),
                    ]
                    .into_iter()
                    .filter(|(n, _, _)| *n > 0)
                    .map(|(n, tone, label)| {
                        h_flex()
                            .gap_1()
                            .child(div().text_color(tone.color(cx)).child(n.to_string()))
                            .child(div().text_color(muted).child(label))
                    }),
                ),
            )
    }
}

impl Render for WorkloadsOverviewView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (background, border, foreground, muted) = (
            theme.background,
            theme.border,
            theme.foreground,
            theme.muted_foreground,
        );
        let counts = self.counts();
        let cards: Vec<_> = counts
            .into_iter()
            .enumerate()
            .map(|(ix, (kind, counts))| self.card(ix, &kind, counts, cx).into_any_element())
            .collect();
        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .text_sm()
            .child(
                h_flex()
                    .w_full()
                    .h_12()
                    .flex_none()
                    .px_4()
                    .gap_3()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .child(div().text_base().child("Workloads"))
                    .child(div().text_color(muted).child(self.namespaces.label()))
                    .when_some(self.error.clone(), |this, e| {
                        this.child(div().text_xs().text_color(Tone::Warning.color(cx)).child(e))
                    }),
            )
            .child(
                div()
                    .id("workloads-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(h_flex().flex_wrap().gap_4().p_4().children(cards)),
            )
    }
}

// ---- cluster ----------------------------------------------------------------

const MAX_WARNINGS: usize = 12;

/// Node count, capacity versus usage (when metrics exist) and recent warning
/// events.
pub struct ClusterOverviewView {
    nodes: ObjectTable,
    events: ObjectTable,
    usage: Option<UsageMap>,
    now: i64,
    error: Option<SharedString>,
    _tasks: Vec<Task<()>>,
}

impl EventEmitter<AppCommand> for ClusterOverviewView {}

impl ClusterOverviewView {
    pub(crate) fn new(backend: &Backend, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let node_printer = printer_for(&kinds::NODE);
        let event_printer = printer_for(&kinds::EVENT);
        let nodes = ObjectTable::new(node_printer.columns().to_vec());
        let events = ObjectTable::new(event_printer.columns().to_vec());

        let mut tasks = vec![
            pump_rows(
                backend.source(&kinds::NODE, &node_printer).as_ref(),
                cx,
                |this: &mut Self, batch, cx| {
                    let (error, changed) = apply(&mut this.nodes, batch);
                    let error = error.map(Into::into).or(this.error.take());
                    if changed || error != this.error {
                        cx.notify();
                    }
                    this.error = error;
                },
            ),
            pump_rows(
                backend.source(&kinds::EVENT, &event_printer).as_ref(),
                cx,
                |this: &mut Self, batch, cx| {
                    if apply(&mut this.events, batch).1 {
                        cx.notify();
                    }
                },
            ),
            tick_ages(
                cx,
                |this: &Self, _| this.events.newest_timestamp(),
                |this: &mut Self, cx| {
                    this.now = unix_now();
                    cx.notify();
                },
            ),
        ];
        if let Some(usage) = backend.usage(&kinds::NODE) {
            tasks.push(pump_usage(
                usage.as_ref(),
                cx,
                |this: &mut Self, usage, cx| {
                    if this.usage.as_ref() != Some(&usage) {
                        this.usage = Some(usage);
                        cx.notify();
                    }
                },
            ));
        }
        Self {
            nodes,
            events,
            usage: None,
            now: unix_now(),
            error: None,
            _tasks: tasks,
        }
    }

    pub fn summary(&self) -> ClusterSummary {
        summarize_cluster(
            self.nodes.all_rows(),
            self.usage.as_ref(),
            self.events.all_rows(),
            MAX_WARNINGS,
        )
    }

    fn gauge(
        &self,
        title: &str,
        capacity: String,
        used: Option<(String, f32)>,
        cx: &App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (panel, border, muted) = (theme.muted, theme.border, theme.muted_foreground);
        let fraction = used.as_ref().map(|(_, f)| f.clamp(0.0, 1.0));
        let tone = match fraction {
            Some(f) if f > 0.9 => Tone::Error,
            Some(f) if f > 0.75 => Tone::Warning,
            _ => Tone::Ok,
        };
        v_flex()
            .w(px(300.))
            .p_4()
            .gap_2()
            .rounded_md()
            .bg(panel)
            .border_1()
            .border_color(border)
            .child(div().text_color(muted).child(title.to_string()))
            .child(div().text_xl().child(capacity))
            .child(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(border)
                    .when_some(fraction, |bar, f| {
                        bar.child(div().h_full().w(relative(f)).bg(tone.color(cx)))
                    }),
            )
            .child(div().text_xs().text_color(muted).child(match used {
                Some((text, _)) => text,
                None => "Metrics unavailable (metrics-server not found)".to_string(),
            }))
    }
}

impl Render for ClusterOverviewView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let summary = self.summary();
        let theme = cx.theme();
        let (background, border, foreground, muted, panel) = (
            theme.background,
            theme.border,
            theme.foreground,
            theme.muted_foreground,
            theme.muted,
        );

        let cpu_used = summary.cpu_usage.map(|used| {
            let fraction = if summary.cpu_capacity > 0.0 {
                (used / summary.cpu_capacity) as f32
            } else {
                0.0
            };
            (
                format!(
                    "{} used of {}",
                    format_cpu(used),
                    format_cpu(summary.cpu_capacity)
                ),
                fraction,
            )
        });
        let mem_used = summary.memory_usage.map(|used| {
            let fraction = if summary.memory_capacity > 0 {
                used as f32 / summary.memory_capacity as f32
            } else {
                0.0
            };
            (
                format!(
                    "{} used of {}",
                    format_memory(used),
                    format_memory(summary.memory_capacity)
                ),
                fraction,
            )
        });

        let nodes_tone = if summary.nodes_ready == summary.nodes_total {
            Tone::Ok
        } else {
            Tone::Error
        };
        let node_card = v_flex()
            .w(px(300.))
            .p_4()
            .gap_2()
            .rounded_md()
            .bg(panel)
            .border_1()
            .border_color(border)
            .child(div().text_color(muted).child("Nodes"))
            .child(
                div()
                    .text_xl()
                    .text_color(nodes_tone.color(cx))
                    .child(format!("{} / {}", summary.nodes_ready, summary.nodes_total)),
            )
            .child(div().text_xs().text_color(muted).child("ready"));

        let warnings = if summary.warnings.is_empty() {
            div()
                .p_4()
                .text_color(muted)
                .child("No warning events")
                .into_any_element()
        } else {
            v_flex()
                .children(summary.warnings.iter().map(|w| {
                    h_flex()
                        .w_full()
                        .px_4()
                        .py_2()
                        .gap_3()
                        .border_b_1()
                        .border_color(border)
                        .child(
                            div()
                                .w(px(48.))
                                .flex_none()
                                .text_color(muted)
                                .child(age_since(w.updated_at, self.now)),
                        )
                        .child(
                            div()
                                .w(px(260.))
                                .flex_none()
                                .truncate()
                                .child(w.object.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(Tone::Warning.color(cx))
                                .child(w.message.clone()),
                        )
                }))
                .into_any_element()
        };

        v_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .text_sm()
            .child(
                h_flex()
                    .w_full()
                    .h_12()
                    .flex_none()
                    .px_4()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .child(div().text_base().child("Cluster"))
                    .when_some(self.error.clone(), |this, e| {
                        this.child(
                            div()
                                .ml_3()
                                .text_xs()
                                .text_color(Tone::Warning.color(cx))
                                .child(e),
                        )
                    }),
            )
            .child(
                v_flex()
                    .id("cluster-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_4()
                            .p_4()
                            .child(node_card)
                            .child(self.gauge(
                                "CPU",
                                format!("{} cores allocatable", format_cpu(summary.cpu_capacity)),
                                cpu_used,
                                cx,
                            ))
                            .child(self.gauge(
                                "Memory",
                                format!("{} allocatable", format_memory(summary.memory_capacity)),
                                mem_used,
                                cx,
                            )),
                    )
                    .child(div().px_4().child(section_header("Warnings", cx)))
                    .child(warnings),
            )
    }
}
