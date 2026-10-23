use std::cmp::Ordering;

use super::pod::PodRow;
use super::source::PodEvent;

/// The pods currently known, kept sorted by namespace then name (the order
/// `kubectl get pods -A` prints).
#[derive(Debug, Clone, Default)]
pub struct PodTable {
    rows: Vec<PodRow>,
}

fn cmp_key(row: &PodRow, namespace: &str, name: &str) -> Ordering {
    row.namespace
        .as_str()
        .cmp(namespace)
        .then_with(|| row.name.as_str().cmp(name))
}

impl PodTable {
    pub fn rows(&self) -> &[PodRow] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Applies one event and returns whether the visible rows changed.
    pub fn apply(&mut self, event: PodEvent) -> bool {
        match event {
            PodEvent::Restarted(mut rows) => {
                rows.sort_by(|a, b| cmp_key(a, &b.namespace, &b.name));
                rows.dedup_by(|a, b| a.namespace == b.namespace && a.name == b.name);
                let changed = rows != self.rows;
                self.rows = rows;
                changed
            }
            PodEvent::Applied(row) => {
                match self
                    .rows
                    .binary_search_by(|probe| cmp_key(probe, &row.namespace, &row.name))
                {
                    Ok(ix) if self.rows[ix] == row => false,
                    Ok(ix) => {
                        self.rows[ix] = row;
                        true
                    }
                    Err(ix) => {
                        self.rows.insert(ix, row);
                        true
                    }
                }
            }
            PodEvent::Deleted { namespace, name } => {
                match self
                    .rows
                    .binary_search_by(|probe| cmp_key(probe, &namespace, &name))
                {
                    Ok(ix) => {
                        self.rows.remove(ix);
                        true
                    }
                    Err(_) => false,
                }
            }
        }
    }
}
