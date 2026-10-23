//! Stable avatar colors derived from a cluster name.

/// A hue in `[0, 1)` derived from the name with FNV-1a, so a cluster keeps the
/// same avatar color across runs (std's hasher is not stable).
pub fn avatar_hue(name: &str) -> f32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash % 360) as f32 / 360.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_is_stable_and_in_range() {
        let h = avatar_hue("dev-cluster");
        assert_eq!(h, avatar_hue("dev-cluster"));
        assert!((0.0..1.0).contains(&h));
    }

    #[test]
    fn different_names_usually_differ() {
        assert_ne!(avatar_hue("dev-cluster"), avatar_hue("prod-cluster"));
    }
}
