//! Desktop composition root: assembles core, providers, and infrastructure
//! behind restricted Tauri IPC. Phase 1 exposes a single synthetic-status
//! command; the full command set lands in task P1-07.

use serde::Serialize;

/// Nonsecret scaffold status shown in the prototype window. Synthetic only.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyntheticStatus {
    provenance: &'static str,
    provider_count: usize,
    store: &'static str,
    message: &'static str,
}

#[tauri::command]
fn synthetic_status() -> SyntheticStatus {
    SyntheticStatus {
        provenance: quotavista_core::Provenance::Synthetic.as_str(),
        provider_count: quotavista_providers::synthetic_providers().len(),
        store: quotavista_infrastructure::STORE_KIND,
        message: "Development scaffold running without provider credentials.",
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![synthetic_status])
        .run(tauri::generate_context!())
        .expect("error while running QuotaVista prototype");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_status_reports_synthetic_provenance() {
        let status = synthetic_status();
        assert_eq!(status.provenance, "synthetic");
        assert_eq!(status.provider_count, 2);
        assert!(status.store.starts_with("synthetic"));
    }
}
