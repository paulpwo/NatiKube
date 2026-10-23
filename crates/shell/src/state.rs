//! Pure shell state: sidebar/tab navigation and the bottom dock's tab list.
//! No GPUI, so it is unit-tested directly.

use std::collections::BTreeSet;

use natikube_core::{
    ResourceKind, SidebarEntry, SidebarSection, ViewId, kinds, section_index, sidebar,
};

/// Which view is shown and which sidebar groups are expanded.
#[derive(Debug, Clone)]
pub struct NavState {
    sections: Vec<SidebarSection>,
    view: ViewId,
    expanded: BTreeSet<usize>,
    /// Expanded API groups inside the Custom Resources section.
    expanded_groups: BTreeSet<String>,
}

impl Default for NavState {
    fn default() -> Self {
        Self::new(ViewId::ClusterOverview)
    }
}

impl NavState {
    pub fn new(view: ViewId) -> Self {
        let mut state = Self {
            sections: sidebar(),
            view: ViewId::ClusterOverview,
            expanded: BTreeSet::new(),
            expanded_groups: BTreeSet::new(),
        };
        state.navigate(view);
        state
    }

    pub fn view(&self) -> &ViewId {
        &self.view
    }

    pub fn sections(&self) -> &[SidebarSection] {
        &self.sections
    }

    /// Mutable access for filling the dynamic Custom Resources section.
    pub fn sections_mut(&mut self) -> &mut Vec<SidebarSection> {
        &mut self.sections
    }

    /// Shows `view` and expands the group that owns it.
    pub fn navigate(&mut self, view: ViewId) {
        self.view = view;
        if let Some(ix) = self.section_index() {
            self.expanded.insert(ix);
        }
        if let ViewId::Resource(kind) = &self.view
            && self.is_custom_kind(kind)
        {
            self.expanded_groups.insert(kind.group.to_string());
        }
    }

    fn is_custom_kind(&self, kind: &ResourceKind) -> bool {
        *kind != kinds::CUSTOM_RESOURCE_DEFINITION
            && self
                .sections
                .iter()
                .find(|s| s.dynamic)
                .is_some_and(|s| s.contains(&ViewId::Resource(kind.clone())))
    }

    /// Discovered custom resource kinds grouped by API group (sorted), without
    /// the Definitions entry.
    pub fn custom_groups(&self) -> Vec<(String, Vec<&SidebarEntry>)> {
        let mut groups: Vec<(String, Vec<&SidebarEntry>)> = Vec::new();
        let Some(section) = self.sections.iter().find(|s| s.dynamic) else {
            return groups;
        };
        for entry in &section.entries {
            let ViewId::Resource(kind) = &entry.view else {
                continue;
            };
            if *kind == kinds::CUSTOM_RESOURCE_DEFINITION {
                continue;
            }
            match groups.iter_mut().find(|(g, _)| g == kind.group.as_ref()) {
                Some((_, list)) => list.push(entry),
                None => groups.push((kind.group.to_string(), vec![entry])),
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        groups
    }

    pub fn is_group_expanded(&self, group: &str) -> bool {
        self.expanded_groups.contains(group)
    }

    pub fn toggle_group(&mut self, group: &str) {
        if !self.expanded_groups.remove(group) {
            self.expanded_groups.insert(group.to_string());
        }
    }

    /// Index of the section owning the current view.
    pub fn section_index(&self) -> Option<usize> {
        section_index(&self.sections, &self.view)
    }

    /// Top tabs: the entries of the current section when it is a group.
    pub fn tabs(&self) -> &[SidebarEntry] {
        match self.section_index().map(|ix| &self.sections[ix]) {
            Some(section) if section.is_group() && !section.dynamic => &section.entries,
            _ => &[],
        }
    }

    pub fn is_expanded(&self, section: usize) -> bool {
        self.expanded.contains(&section)
    }

    pub fn toggle_section(&mut self, section: usize) {
        if !self.expanded.remove(&section) {
            self.expanded.insert(section);
        }
    }
}

/// Which dock tab is active and whether the dock is visible. The tab
/// contents live in the shell view; this tracks indices only.
#[derive(Debug, Clone, Default)]
pub struct DockState {
    len: usize,
    active: Option<usize>,
    open: bool,
}

impl DockState {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// Adds a tab at the end, activates it and opens the dock. Returns its index.
    pub fn push(&mut self) -> usize {
        let ix = self.len;
        self.len += 1;
        self.active = Some(ix);
        self.open = true;
        ix
    }

    pub fn activate(&mut self, ix: usize) {
        if ix < self.len {
            self.active = Some(ix);
        }
    }

    /// Removes tab `ix`, keeping a valid active tab; closes the dock when the
    /// last tab goes.
    pub fn close(&mut self, ix: usize) {
        if ix >= self.len {
            return;
        }
        self.len -= 1;
        if self.len == 0 {
            self.active = None;
            self.open = false;
            return;
        }
        self.active = match self.active {
            Some(active) if active > ix => Some(active - 1),
            Some(active) if active == ix => Some(ix.saturating_sub(1)),
            other => other,
        };
    }

    /// Shows or hides the dock; ignored when there are no tabs.
    pub fn toggle(&mut self) {
        if self.len > 0 {
            self.open = !self.open;
        }
    }
}
