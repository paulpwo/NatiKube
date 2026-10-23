//! Pure forward registry, port parsing and service-to-pod resolution.

mod open;
mod ports;
mod registry;
mod resolve;

pub use open::{open_url, open_url_command};
pub use ports::{PortError, format_local_url, parse_port, parse_port_pair};
pub use registry::{ForwardEntry, ForwardRegistry, ForwardState};
pub use resolve::{ResolveError, label_selector, pick_pod, target_port};
