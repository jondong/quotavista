//! Provider adapters. Deterministic synthetic demo adapters plus the
//! OpenRouter and Kimi live-adapter skeletons (query-only, fixture-tested
//! offline).
pub mod kimi;
pub mod openrouter;
pub mod simulated;
use quotavista_core::Provenance;

/// Nonsecret description of a demo provider account.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntheticProvider {
    /// Stable synthetic identifier, never a real account.
    pub id: &'static str,
    /// Display name for the demo entry.
    pub display_name: &'static str,
    /// Always [`Provenance::Synthetic`] in Phase 1.
    pub provenance: Provenance,
}

/// The two demo providers standing in for the planned integrations.
pub fn synthetic_providers() -> [SyntheticProvider; 2] {
    [
        SyntheticProvider {
            id: "synthetic-kimi",
            display_name: "Kimi (synthetic)",
            provenance: Provenance::Synthetic,
        },
        SyntheticProvider {
            id: "synthetic-openrouter",
            display_name: "OpenRouter (synthetic)",
            provenance: Provenance::Synthetic,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_providers_are_marked_synthetic() {
        let providers = synthetic_providers();
        assert_eq!(providers.len(), 2);
        assert!(providers
            .iter()
            .all(|p| p.provenance == Provenance::Synthetic));
    }
}
