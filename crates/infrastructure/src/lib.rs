//! Infrastructure boundaries: persistence, secrets, and network.
//!
//! Phase 1 scaffold: no SQLite, secret store, or HTTP implementation yet.
//! SQLx arrives in task P1-03; when it is added, pin a SQLite release with
//! the upstream WAL fixes and verify the linked runtime version at setup
//! (see CONTRIBUTING.md).

/// Kind of the only store available in this scaffold. Development defaults to
/// synthetic in-memory state; nothing touches a real application data dir.
pub const STORE_KIND: &str = "synthetic-in-memory";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_store_is_synthetic() {
        assert!(STORE_KIND.starts_with("synthetic"));
    }
}
