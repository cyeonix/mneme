use crate::provider_config::{ProviderEnv, SharedProviderSecretStore};
use crate::state::{DaemonProcessState, DesktopDaemonStatus};
use std::fmt;
use std::sync::{Arc, Mutex};
use thiserror::Error;

const DEFAULT_DAEMON_BIND_ADDR: &str = "127.0.0.1:3766";

#[derive(Clone, PartialEq, Eq)]
pub struct DaemonLaunchConfig {
    bind_addr: String,
    env: Vec<(String, String)>,
}

impl fmt::Debug for DaemonLaunchConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redacted_debug())
    }
}

impl DaemonLaunchConfig {
    pub fn new(bind_addr: String, provider: ProviderEnv) -> Self {
        let mut env = vec![
            ("MNEME_BIND_ADDR".to_string(), bind_addr.clone()),
            ("MNEME_OPENAI_BASE_URL".to_string(), provider.base_url),
            ("MNEME_OPENAI_MODEL".to_string(), provider.model),
            (
                "MNEME_OPENAI_TIMEOUT_SECS".to_string(),
                provider.timeout_secs.to_string(),
            ),
        ];
        if let Some(api_key) = provider.api_key {
            env.push(("MNEME_OPENAI_API_KEY".to_string(), api_key));
        }
        Self { bind_addr, env }
    }

    pub fn api_base_url(&self) -> String {
        format!("http://{}", self.bind_addr)
    }

    #[allow(dead_code)]
    pub fn env_value(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn redacted_debug(&self) -> String {
        let env = self
            .env
            .iter()
            .map(|(key, value)| {
                if key == "MNEME_OPENAI_API_KEY" {
                    format!("{key}=<redacted>")
                } else {
                    format!("{key}={value}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "DaemonLaunchConfig {{ bind_addr: {}, env: [{}] }}",
            self.bind_addr, env
        )
    }
}

#[derive(Debug, Error)]
pub enum DaemonSupervisorError {
    #[error("daemon supervisor lock is poisoned")]
    Poisoned,
    #[error("daemon failed to start: {0}")]
    Start(String),
}

pub trait DaemonSupervisor: fmt::Debug + Send + Sync {
    fn ensure_started(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError>;
    fn restart(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError>;
    fn status(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError>;
}

pub type SharedDaemonSupervisor = Arc<dyn DaemonSupervisor>;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug)]
pub struct InMemoryDaemonSupervisor {
    status: Mutex<DesktopDaemonStatus>,
}

impl Default for InMemoryDaemonSupervisor {
    fn default() -> Self {
        Self {
            status: Mutex::new(not_started_status(DEFAULT_DAEMON_BIND_ADDR)),
        }
    }
}

#[cfg(test)]
impl InMemoryDaemonSupervisor {
    pub fn shared(self) -> SharedDaemonSupervisor {
        Arc::new(self)
    }
}

impl DaemonSupervisor for InMemoryDaemonSupervisor {
    fn ensure_started(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        let mut status = self
            .status
            .lock()
            .map_err(|_| DaemonSupervisorError::Poisoned)?;
        status.state = DaemonProcessState::Ready;
        status.api_base_url = Some(format!("http://{DEFAULT_DAEMON_BIND_ADDR}"));
        status.last_error = None;
        Ok(status.clone())
    }

    fn restart(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        self.ensure_started()
    }

    fn status(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        Ok(self
            .status
            .lock()
            .map_err(|_| DaemonSupervisorError::Poisoned)?
            .clone())
    }
}

#[derive(Debug)]
pub struct SidecarDaemonSupervisor {
    bind_addr: String,
    provider_store: SharedProviderSecretStore,
    status: Mutex<DesktopDaemonStatus>,
}

impl SidecarDaemonSupervisor {
    pub fn new(provider_store: SharedProviderSecretStore) -> Self {
        let bind_addr = DEFAULT_DAEMON_BIND_ADDR.to_string();
        Self {
            status: Mutex::new(not_started_status(&bind_addr)),
            bind_addr,
            provider_store,
        }
    }

    pub fn shared(self) -> SharedDaemonSupervisor {
        Arc::new(self)
    }

    fn launch_config(&self) -> Result<DaemonLaunchConfig, DaemonSupervisorError> {
        let provider = self
            .provider_store
            .load_provider_env()
            .map_err(|error| DaemonSupervisorError::Start(error.to_string()))?;
        Ok(DaemonLaunchConfig::new(self.bind_addr.clone(), provider))
    }
}

impl DaemonSupervisor for SidecarDaemonSupervisor {
    fn ensure_started(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        let launch_config = self.launch_config()?;
        let mut status = self
            .status
            .lock()
            .map_err(|_| DaemonSupervisorError::Poisoned)?;
        status.state = DaemonProcessState::Ready;
        status.api_base_url = Some(launch_config.api_base_url());
        status.last_error = None;
        Ok(status.clone())
    }

    fn restart(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        let mut status = self
            .status
            .lock()
            .map_err(|_| DaemonSupervisorError::Poisoned)?;
        status.state = DaemonProcessState::Stopped;
        drop(status);
        self.ensure_started()
    }

    fn status(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        Ok(self
            .status
            .lock()
            .map_err(|_| DaemonSupervisorError::Poisoned)?
            .clone())
    }
}

fn not_started_status(bind_addr: &str) -> DesktopDaemonStatus {
    DesktopDaemonStatus {
        state: DaemonProcessState::NotStarted,
        api_base_url: Some(format!("http://{bind_addr}")),
        last_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_config::{
        DesktopProviderConfigInput, InMemoryProviderSecretStore, ProviderEnv,
    };

    #[test]
    fn sidecar_env_includes_daemon_and_provider_values_without_exposing_key_in_debug() {
        let env = ProviderEnv {
            base_url: "https://provider.example/v1".to_string(),
            api_key: Some("secret-key".to_string()),
            model: "gpt-test".to_string(),
            timeout_secs: 45,
        };
        let command = DaemonLaunchConfig::new("127.0.0.1:3766".to_string(), env);

        assert_eq!(command.env_value("MNEME_BIND_ADDR"), Some("127.0.0.1:3766"));
        assert_eq!(
            command.env_value("MNEME_OPENAI_BASE_URL"),
            Some("https://provider.example/v1")
        );
        assert_eq!(
            command.env_value("MNEME_OPENAI_API_KEY"),
            Some("secret-key")
        );
        assert_eq!(command.env_value("MNEME_OPENAI_MODEL"), Some("gpt-test"));
        assert_eq!(command.env_value("MNEME_OPENAI_TIMEOUT_SECS"), Some("45"));
        assert!(command
            .redacted_debug()
            .contains("MNEME_OPENAI_API_KEY=<redacted>"));
        assert!(!command.redacted_debug().contains("secret-key"));
        assert!(!format!("{command:?}").contains("secret-key"));
    }

    #[test]
    fn in_memory_supervisor_transitions_to_ready() {
        let supervisor = InMemoryDaemonSupervisor::default().shared();
        let status = supervisor.ensure_started().expect("start daemon");
        assert_eq!(status.state, DaemonProcessState::Ready);
        assert_eq!(
            status.api_base_url.as_deref(),
            Some("http://127.0.0.1:3766")
        );
    }

    #[test]
    fn sidecar_supervisor_loads_provider_env_without_exposing_key_in_debug() {
        let provider_store = InMemoryProviderSecretStore::default().shared();
        provider_store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "sidecar-secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");
        let supervisor = SidecarDaemonSupervisor::new(provider_store);

        let status = supervisor.ensure_started().expect("start daemon");

        assert_eq!(status.state, DaemonProcessState::Ready);
        assert_eq!(
            status.api_base_url.as_deref(),
            Some("http://127.0.0.1:3766")
        );
        assert!(!format!("{supervisor:?}").contains("sidecar-secret-key"));
    }
}
