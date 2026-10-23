use natikube_core::PortForwardSpec;

/// Lifecycle of one forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardState {
    /// Requested, not listening yet.
    Pending,
    /// Listening on `local_port`.
    Active {
        local_port: u16,
    },
    Failed(String),
    Stopped,
}

impl ForwardState {
    /// Still holds (or is about to hold) a local port.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Pending | Self::Active { .. })
    }
}

/// One row in the port-forwards list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardEntry {
    pub id: u64,
    pub spec: PortForwardSpec,
    pub state: ForwardState,
}

/// Ordered list of forwards with stable ids.
#[derive(Debug, Default)]
pub struct ForwardRegistry {
    entries: Vec<ForwardEntry>,
    next_id: u64,
}

impl ForwardRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, spec: PortForwardSpec) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.entries.push(ForwardEntry {
            id,
            spec,
            state: ForwardState::Pending,
        });
        id
    }

    pub fn entries(&self) -> &[ForwardEntry] {
        &self.entries
    }

    pub fn get(&self, id: u64) -> Option<&ForwardEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Updates the state; returns `false` for an unknown id. A stopped forward
    /// never becomes active again (a late "active" report is ignored).
    pub fn set_state(&mut self, id: u64, state: ForwardState) -> bool {
        match self.entries.iter_mut().find(|e| e.id == id) {
            Some(entry) => {
                if entry.state == ForwardState::Stopped {
                    return true;
                }
                entry.state = state;
                true
            }
            None => false,
        }
    }

    /// Removes a forward that is no longer live; live ones must be stopped first.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| !(e.id == id && !e.state.is_live()));
        self.entries.len() != before
    }

    pub fn live_count(&self) -> usize {
        self.entries.iter().filter(|e| e.state.is_live()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use natikube_core::{ClusterId, ResourceRef, kinds};

    fn spec(port: u16) -> PortForwardSpec {
        PortForwardSpec {
            target: ResourceRef::new(ClusterId::new("c"), kinds::POD, Some("ns".into()), "p"),
            remote_port: port,
            local_port: None,
        }
    }

    #[test]
    fn ids_are_stable_and_increasing() {
        let mut r = ForwardRegistry::new();
        let a = r.add(spec(80));
        let b = r.add(spec(81));
        assert_eq!((a, b), (1, 2));
        r.remove(a);
        assert_eq!(r.add(spec(82)), 3);
    }

    #[test]
    fn live_forwards_cannot_be_removed() {
        let mut r = ForwardRegistry::new();
        let id = r.add(spec(80));
        assert!(!r.remove(id));
        r.set_state(id, ForwardState::Stopped);
        assert!(r.remove(id));
        assert!(r.is_empty());
    }

    #[test]
    fn stopped_stays_stopped() {
        let mut r = ForwardRegistry::new();
        let id = r.add(spec(80));
        r.set_state(id, ForwardState::Stopped);
        r.set_state(id, ForwardState::Active { local_port: 9 });
        assert_eq!(r.get(id).unwrap().state, ForwardState::Stopped);
        assert_eq!(r.live_count(), 0);
    }

    #[test]
    fn unknown_id_is_reported() {
        let mut r = ForwardRegistry::new();
        assert!(!r.set_state(42, ForwardState::Stopped));
    }
}
