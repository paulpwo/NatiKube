//! The Overview tab: properties, labels, conditions, containers, data, events.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use natikube_core::{AppCommand, PortForwardSpec, ResourceRef, kinds};
use natikube_ui_kit::{link_text, section_header, status_dot, status_text};

use super::panel::{DetailPanel, unix_now};
use super::widgets::{card, chip, clip, prop_row, tone, wrap_row};
use crate::domain::model::{ContainerInfo, ContainerPort, Field, ObjectLink};
use crate::domain::secret::display_value;

/// Longest annotation / data value rendered inline.
const MAX_INLINE: usize = 600;

impl DetailPanel {
    fn link_ref(&self, link: &ObjectLink) -> ResourceRef {
        let namespace = self
            .target
            .as_ref()
            .and_then(|t| t.namespace.clone())
            .filter(|_| link.kind.namespaced);
        ResourceRef::new(
            self.connection.id().clone(),
            link.kind.clone(),
            namespace,
            link.name.clone(),
        )
    }

    fn object_link(
        &self,
        id: impl Into<ElementId>,
        link: &ObjectLink,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let target = self.link_ref(link);
        link_text(
            id,
            link.name.clone(),
            cx.listener(move |_, _, _, cx| cx.emit(AppCommand::OpenDetail(target.clone()))),
            cx,
        )
    }

    fn field_row(&self, index: usize, field: &Field, cx: &mut Context<Self>) -> Div {
        let value: AnyElement = if let Some(link) = &field.link {
            self.object_link(("detail-field-link", index), link, cx)
                .into_any_element()
        } else if let Some(health) = field.health {
            status_text(field.value.clone(), tone(health), cx).into_any_element()
        } else {
            div().child(field.value.clone()).into_any_element()
        };
        prop_row(field.label.clone(), value, cx)
    }

    pub(super) fn render_overview(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let now = unix_now();
        let mut col = v_flex().w_full().gap_1().p_3();
        let Some(model) = self.model.clone() else {
            return div().id("detail-overview").size_full().child(col);
        };
        let muted = cx.theme().muted_foreground;

        // Properties
        col = col.child(section_header("Properties", cx));
        for (i, field) in model.metadata.iter().enumerate() {
            let shown = if field.label == "Created" {
                Field {
                    value: format!("{} ago ({})", model.age(now), field.value),
                    ..field.clone()
                }
            } else {
                field.clone()
            };
            col = col.child(self.field_row(i, &shown, cx));
        }
        if !model.owners.is_empty() {
            let mut links = v_flex().gap_0p5();
            for (i, owner) in model.owners.iter().enumerate() {
                links = links.child(
                    h_flex()
                        .gap_1()
                        .child(div().text_color(muted).child(owner.kind.kind.to_string()))
                        .child(self.object_link(("detail-owner", i), owner, cx)),
                );
            }
            col = col.child(prop_row("Controlled by", links, cx));
        }
        for (i, field) in model.status.iter().enumerate() {
            col = col.child(self.field_row(1000 + i, field, cx));
        }

        // Labels and annotations
        if !model.labels.is_empty() {
            col = col.child(section_header("Labels", cx)).child(
                wrap_row().children(
                    model
                        .labels
                        .iter()
                        .map(|(k, v)| chip(format!("{k}={}", clip(v, 80)), cx)),
                ),
            );
        }
        if !model.annotations.is_empty() {
            col = col.child(section_header("Annotations", cx)).child(
                wrap_row().children(
                    model
                        .annotations
                        .iter()
                        .map(|(k, v)| chip(format!("{k}: {}", clip(v, 120)), cx)),
                ),
            );
        }

        // Conditions
        if !model.conditions.is_empty() {
            col = col.child(section_header("Conditions", cx));
            let mut table = v_flex().w_full().gap_1();
            table = table.child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .text_xs()
                    .text_color(muted)
                    .child(div().w(px(120.)).flex_none().child("Type"))
                    .child(div().w(px(56.)).flex_none().child("Status"))
                    .child(div().flex_1().child("Reason"))
                    .child(div().w(px(48.)).flex_none().child("Age")),
            );
            for c in &model.conditions {
                table = table.child(
                    v_flex()
                        .w_full()
                        .child(
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_start()
                                .child(div().w(px(120.)).flex_none().child(c.kind.clone()))
                                .child(
                                    status_text(c.status.clone(), tone(c.health), cx)
                                        .w(px(56.))
                                        .flex_none(),
                                )
                                .child(div().flex_1().min_w_0().child(c.reason.clone()))
                                .child(
                                    div()
                                        .w(px(48.))
                                        .flex_none()
                                        .text_color(muted)
                                        .child(c.age(now)),
                                ),
                        )
                        .children((!c.message.is_empty()).then(|| {
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(clip(&c.message, MAX_INLINE))
                        })),
                );
            }
            col = col.child(table);
        }

        // Containers
        if !model.containers.is_empty() {
            col = col.child(section_header("Containers", cx));
            for (i, container) in model.containers.iter().enumerate() {
                col = col.child(self.render_container(i, container, cx));
            }
        }

        // Service ports
        if !model.ports.is_empty() {
            col = col.child(section_header("Ports", cx));
            for (i, port) in model.ports.iter().enumerate() {
                col = col.child(self.port_row(("detail-svc-port", i), port, cx));
            }
        }

        // ConfigMap data
        if !model.data.is_empty() {
            col = col.child(section_header("Data", cx));
            for (key, value) in &model.data {
                col = col.child(
                    card(cx)
                        .child(div().font_weight(FontWeight::MEDIUM).child(key.clone()))
                        .child(div().text_xs().text_color(muted).child(clip(value, 2000))),
                );
            }
        }

        // Secret data
        if !model.secret.is_empty() {
            col = col.child(section_header("Data", cx));
            for (i, entry) in model.secret.iter().enumerate() {
                let revealed = self.revealed.contains(&entry.key);
                let key = entry.key.clone();
                let toggle_key = entry.key.clone();
                let copy_value = entry.decoded();
                col = col.child(
                    card(cx)
                        .child(
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(key),
                                )
                                .child(
                                    Button::new(("detail-secret-toggle", i))
                                        .label(if revealed { "Hide" } else { "Reveal" })
                                        .small()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if !this.revealed.remove(&toggle_key) {
                                                this.revealed.insert(toggle_key.clone());
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new(("detail-secret-copy", i))
                                        .label("Copy")
                                        .small()
                                        .disabled(copy_value.is_none())
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            if let Some(value) = &copy_value {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    value.clone(),
                                                ));
                                            }
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(clip(&display_value(entry, revealed), 2000)),
                        ),
                );
            }
        }

        // Events
        col = col.child(section_header("Events", cx));
        if self.events.is_empty() {
            col = col.child(div().text_color(muted).child("No events"));
        }
        for event in &self.events {
            col = col.child(
                card(cx)
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .child(
                                status_text(event.reason.clone(), tone(event.health), cx).flex_1(),
                            )
                            .child(div().text_xs().text_color(muted).child(format!(
                                "x{}  {}",
                                event.count,
                                event.age(now)
                            ))),
                    )
                    .child(div().text_xs().child(clip(&event.message, MAX_INLINE)))
                    .children((!event.source.is_empty()).then(|| {
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(event.source.clone())
                    })),
            );
        }

        div()
            .id("detail-overview")
            .size_full()
            .overflow_y_scroll()
            .child(col)
    }

    fn port_row(
        &self,
        id: (&'static str, usize),
        port: &ContainerPort,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(div().flex_1().min_w_0().child(port.label.clone()));
        if let Some(target) = self.target.clone()
            && (target.kind == kinds::POD || target.kind == kinds::SERVICE)
        {
            let remote_port = port.number;
            row = row.child(link_text(
                id,
                "Forward",
                cx.listener(move |_, _, _, cx| {
                    cx.emit(AppCommand::StartPortForward(PortForwardSpec {
                        target: target.clone(),
                        remote_port,
                        local_port: None,
                    }))
                }),
                cx,
            ));
        }
        row
    }

    fn render_container(&self, index: usize, c: &ContainerInfo, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut head = h_flex().w_full().gap_2().items_center();
        if let Some((_, health)) = &c.state {
            head = head.child(status_dot(tone(*health), cx));
        }
        head = head.child(div().font_weight(FontWeight::MEDIUM).child(c.name.clone()));
        if c.init {
            head = head.child(chip("init", cx));
        }
        if let Some((label, health)) = &c.state {
            head = head.child(status_text(label.clone(), tone(*health), cx));
        }
        head = head.child(div().flex_1());
        if self.is_pod()
            && let Some(pod) = self.target.clone()
        {
            let (logs_pod, exec_pod) = (pod.clone(), pod);
            let (logs_name, exec_name) = (c.name.clone(), c.name.clone());
            head = head
                .child(
                    Button::new(("detail-c-logs", index))
                        .label("Logs")
                        .small()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(AppCommand::OpenLogs {
                                pod: logs_pod.clone(),
                                container: Some(logs_name.clone()),
                            })
                        })),
                )
                .child(
                    Button::new(("detail-c-shell", index))
                        .label("Shell")
                        .small()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(AppCommand::OpenExec {
                                pod: exec_pod.clone(),
                                container: Some(exec_name.clone()),
                            })
                        })),
                );
        }

        let mut card = card(cx)
            .child(head)
            .child(div().text_xs().text_color(muted).child(c.image.clone()));
        if let Some(restarts) = c.restarts {
            card = card.child(prop_row("Restarts", div().child(restarts.to_string()), cx));
        }
        if !c.ports.is_empty() {
            let mut ports = v_flex().gap_0p5();
            for (i, port) in c.ports.iter().enumerate() {
                ports = ports.child(self.port_row(("detail-c-port", index * 100 + i), port, cx));
            }
            card = card.child(prop_row("Ports", ports, cx));
        }
        if !c.env.is_empty() {
            card = card.child(prop_row(
                "Environment",
                wrap_row().children(c.env.iter().map(|name| chip(name.clone(), cx))),
                cx,
            ));
        }
        if !c.mounts.is_empty() {
            card = card.child(prop_row(
                "Mounts",
                v_flex().children(c.mounts.iter().map(|m| div().child(m.clone()))),
                cx,
            ));
        }
        for (kind, text) in &c.probes {
            card = card.child(prop_row(
                kind.clone(),
                div().text_xs().child(text.clone()),
                cx,
            ));
        }
        for (kind, text) in &c.resources {
            card = card.child(prop_row(kind.clone(), div().child(text.clone()), cx));
        }
        card
    }
}
