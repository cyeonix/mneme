use crate::daemon::{SharedDaemonSupervisor, SidecarDaemonSupervisor};
use crate::provider_config::{
    DesktopProviderConfigSummary, SharedProviderSecretStore, StrongholdReadyProviderSecretStore,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonProcessState {
    NotStarted,
    Starting,
    Ready,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDaemonStatus {
    pub state: DaemonProcessState,
    pub api_base_url: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopRuntimeStatus {
    pub daemon: DesktopDaemonStatus,
    pub provider: DesktopProviderConfigSummary,
}

#[derive(Debug)]
pub struct AppState {
    pub api_base_url: String,
    pub daemon_supervisor: SharedDaemonSupervisor,
    pub provider_store: SharedProviderSecretStore,
}

impl AppState {
    pub fn new() -> Self {
        let api_base_url = "http://127.0.0.1:3766".to_string();
        let provider_store = StrongholdReadyProviderSecretStore::new().shared();
        let daemon_supervisor = SidecarDaemonSupervisor::new(provider_store.clone()).shared();
        Self {
            api_base_url: api_base_url.clone(),
            daemon_supervisor,
            provider_store,
        }
    }
}
