//! Provider selection for refresh composition.
//!
//! [`ProviderRouter`] is the single [`ProviderPort`] handed to the refresh
//! coordinator. Each call resolves the connection row through the repository
//! and dispatches on its stored provider id:
//!
//! - `synthetic-kimi` / `synthetic-openrouter` connections stay on the
//!   simulated demo adapters and never touch a live transport. When the
//!   simulated router was injected as a single-adapter override (tests), every
//!   connection routes to it.
//! - `kimi` / `openrouter` connections build the P2-02/P2-03 live adapter for
//!   the call, resolving the bearer token from the stored credential reference
//!   through the secret store. Any credential-resolution failure (no reference,
//!   unreadable store, invalid material, degraded composition without secrets)
//!   degrades to the adapter's own missing-token `PermissionRequired` failure,
//!   which never issues a provider request.
//! - An unknown provider id, a vanished connection row, or a local storage
//!   error fails typed as `InvalidResponse` without any network traffic. A
//!   live provider whose transport could not be constructed in this
//!   composition fails typed as `Offline`.
//!
//! History is delegated the same way; the live adapters currently serve no
//! history (`Ok(None)`), so their jobs complete with `HistoryOutcome::Skipped`.

use std::sync::Arc;

use quotavista_core::{
    provider::{DatasetCurrent, HistoryPage, ProviderError, ProviderPort, ProviderRequest},
    secrets::SecretStore,
    storage::Repository,
};
use quotavista_providers::{
    kimi::{KimiMembershipAdapter, KimiTransport, SessionTokenSource},
    openrouter::{AuthTokenSource, BearerToken, OpenRouterAdapter, OpenRouterTransport},
    simulated::SimulatedRouter,
};

/// Stored provider id for live Kimi connections (see `connections::provider_id`).
const LIVE_KIMI: &str = "kimi";
/// Stored provider id for live OpenRouter connections.
const LIVE_OPENROUTER: &str = "openrouter";
/// Any provider id starting with this prefix is a simulated demo connection.
const SYNTHETIC_PREFIX: &str = "synthetic";

/// Per-call credential material. A `None` token lets the live adapter fail
/// typed (`PermissionRequired`) without issuing a provider request.
struct ResolvedToken(Option<BearerToken>);

impl AuthTokenSource for ResolvedToken {
    fn management_token(&self) -> Option<BearerToken> {
        self.0.clone()
    }
}

impl SessionTokenSource for ResolvedToken {
    fn session_token(&self) -> Option<BearerToken> {
        self.0.clone()
    }
}

/// Where one provider call goes.
enum Resolution<OT, KT> {
    /// Test override: the injected single simulated adapter answers everything.
    SimulatedOverride,
    SyntheticKimi,
    SyntheticOpenrouter,
    LiveKimi(KT, ResolvedToken),
    LiveOpenrouter(OT, ResolvedToken),
    /// Typed failure produced before any network traffic.
    Failed(ProviderError),
}

/// Selects the provider adapter for each refresh call from stored connection
/// and secret references.
pub struct ProviderRouter<R, S, OT, KT> {
    repository: R,
    secrets: Option<Arc<S>>,
    openrouter_transport: Option<OT>,
    kimi_transport: Option<KT>,
    simulated: SimulatedRouter,
}

impl<R, S, OT, KT> ProviderRouter<R, S, OT, KT> {
    /// `secrets` is `None` in a degraded composition; live connections then
    /// fail typed without network while synthetic connections keep working.
    /// A `None` transport (client construction failed) marks that provider's
    /// live connections `Offline`.
    pub fn new(
        repository: R,
        secrets: Option<Arc<S>>,
        openrouter_transport: Option<OT>,
        kimi_transport: Option<KT>,
        simulated: SimulatedRouter,
    ) -> Self {
        Self {
            repository,
            secrets,
            openrouter_transport,
            kimi_transport,
            simulated,
        }
    }
}

impl<R, S, OT, KT> ProviderRouter<R, S, OT, KT>
where
    R: Repository + Send + Sync,
    S: SecretStore + 'static,
    OT: OpenRouterTransport + Clone,
    KT: KimiTransport + Clone,
{
    async fn resolve(&self, connection_id: &str) -> Resolution<OT, KT> {
        if self.simulated.is_single() {
            return Resolution::SimulatedOverride;
        }
        let connection = match self.repository.connection(connection_id).await {
            Ok(Some(connection)) => connection,
            // A vanished row or a local storage failure is not a provider
            // response; fail typed without touching the network.
            _ => return Resolution::Failed(ProviderError::InvalidResponse),
        };
        let provider = connection.provider.as_str();
        match provider {
            LIVE_KIMI => match &self.kimi_transport {
                Some(transport) => {
                    Resolution::LiveKimi(transport.clone(), self.token(connection_id).await)
                }
                None => Resolution::Failed(ProviderError::Offline),
            },
            LIVE_OPENROUTER => match &self.openrouter_transport {
                Some(transport) => {
                    Resolution::LiveOpenrouter(transport.clone(), self.token(connection_id).await)
                }
                None => Resolution::Failed(ProviderError::Offline),
            },
            // Any synthetic provider id stays on the demo adapters; the side
            // follows the kimi substring, mirroring `SimulatedRouter::route`,
            // so legacy fixtures like `synthetic-provider-…` keep working.
            _ if provider.starts_with(SYNTHETIC_PREFIX) => {
                if provider.contains(LIVE_KIMI) {
                    Resolution::SyntheticKimi
                } else {
                    Resolution::SyntheticOpenrouter
                }
            }
            _ => Resolution::Failed(ProviderError::InvalidResponse),
        }
    }

    /// Resolve the bearer token for one live connection. The store read may
    /// block (OS keychain), so it runs off the async runtime's executor. Every
    /// failure mode collapses to a missing token: the adapter then fails typed
    /// as `PermissionRequired` and the UI offers credential entry.
    async fn token(&self, connection_id: &str) -> ResolvedToken {
        let Some(secrets) = self.secrets.clone() else {
            return ResolvedToken(None);
        };
        let Ok(Some(reference)) = self.repository.credential_ref(connection_id).await else {
            return ResolvedToken(None);
        };
        let read = tokio::task::spawn_blocking(move || secrets.read(&reference)).await;
        match read {
            Ok(Ok(secret)) => ResolvedToken(BearerToken::new(&secret).ok()),
            _ => ResolvedToken(None),
        }
    }
}

impl<R, S, OT, KT> ProviderPort for ProviderRouter<R, S, OT, KT>
where
    R: Repository + Send + Sync,
    S: SecretStore + 'static,
    OT: OpenRouterTransport + Clone,
    KT: KimiTransport + Clone,
{
    async fn current(
        &self,
        request: &ProviderRequest,
    ) -> Result<Vec<DatasetCurrent>, ProviderError> {
        match self.resolve(&request.connection_id).await {
            Resolution::SimulatedOverride => self.simulated.current(request).await,
            Resolution::SyntheticKimi => self.simulated.kimi().current(request).await,
            Resolution::SyntheticOpenrouter => self.simulated.openrouter().current(request).await,
            Resolution::LiveKimi(transport, token) => {
                KimiMembershipAdapter::new(transport, token)
                    .current(request)
                    .await
            }
            Resolution::LiveOpenrouter(transport, token) => {
                OpenRouterAdapter::new(transport, token)
                    .current(request)
                    .await
            }
            Resolution::Failed(error) => Err(error),
        }
    }

    async fn history_page(
        &self,
        request: &ProviderRequest,
        cursor: Option<&str>,
    ) -> Result<Option<HistoryPage>, ProviderError> {
        match self.resolve(&request.connection_id).await {
            Resolution::SimulatedOverride => self.simulated.history_page(request, cursor).await,
            Resolution::SyntheticKimi => self.simulated.kimi().history_page(request, cursor).await,
            Resolution::SyntheticOpenrouter => {
                self.simulated
                    .openrouter()
                    .history_page(request, cursor)
                    .await
            }
            Resolution::LiveKimi(transport, token) => {
                KimiMembershipAdapter::new(transport, token)
                    .history_page(request, cursor)
                    .await
            }
            Resolution::LiveOpenrouter(transport, token) => {
                OpenRouterAdapter::new(transport, token)
                    .history_page(request, cursor)
                    .await
            }
            Resolution::Failed(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests;
