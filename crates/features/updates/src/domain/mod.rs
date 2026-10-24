mod policy;
mod release;

pub use policy::{CheckContext, ENV_DISABLE, should_check};
pub use release::{
    Release, UpdateOffer, available_update, newest_release, notes_excerpt, parse_tag,
};
