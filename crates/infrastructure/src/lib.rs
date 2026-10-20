//! Infrastructure adapters for backend-owned boundaries.
pub mod durable;
pub mod lifecycle;
pub mod refresh;
pub mod secrets;
pub mod sqlite;

/// The desktop scaffold remains in-memory until the composition task wires the
/// P1-04 lifecycle coordinator to an explicitly opened SQLite store. P1-03
/// provides the adapter and P1-04 the recovery fences; neither is composed yet.
pub const STORE_KIND: &str = "synthetic-in-memory";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_store_is_synthetic() {
        assert!(STORE_KIND.starts_with("synthetic"));
    }
}
