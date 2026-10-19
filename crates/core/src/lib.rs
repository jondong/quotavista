//! QuotaVista domain rules and application ports.
//!
//! Phase 1 scaffold: only synthetic provenance is defined here. Domain
//! primitives and checked IPC contracts arrive in task P1-02.

/// Origin of a displayed value. Phase 1 serves synthetic data only, and every
/// demo value must carry this marker so it can never pass as a real reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Provenance {
    /// Deterministic demo data generated locally, never from a provider.
    Synthetic,
}

impl Provenance {
    /// Stable string form used across the IPC boundary.
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::Synthetic => "synthetic",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_provenance_has_stable_label() {
        assert_eq!(Provenance::Synthetic.as_str(), "synthetic");
    }
}
