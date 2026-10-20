//! Infrastructure adapters for backend-owned boundaries.
pub mod sqlite;

/// The desktop scaffold remains in-memory until the composition task installs
/// recovery fences and explicitly opens SQLite. P1-03 provides the adapter only.
pub const STORE_KIND: &str = "synthetic-in-memory";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_store_is_synthetic() {
        assert!(STORE_KIND.starts_with("synthetic"));
    }
}
