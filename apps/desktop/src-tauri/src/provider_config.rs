use serde::{Deserialize, Serialize};
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_OPENAI_TIMEOUT_SECS: u64 = 30;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConfigInput {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl Debug for DesktopProviderConfigInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesktopProviderConfigInput")
            .field("base_url", &self.base_url)
            .field(
                "api_key",
                &redacted_secret_presence(!self.api_key.trim().is_empty()),
            )
            .field("model", &self.model)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConfigSummary {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
    pub has_api_key: bool,
}

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderEnv {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u64,
}

#[allow(dead_code)]
impl ProviderEnv {
    pub fn redacted_debug(&self) -> String {
        format!(
            "ProviderEnv {{ base_url: {}, api_key: {}, model: {}, timeout_secs: {} }}",
            self.base_url,
            redacted_secret_presence(self.api_key.is_some()),
            self.model,
            self.timeout_secs
        )
    }
}

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("provider secret store lock is poisoned")]
    Poisoned,
}

pub trait ProviderSecretStore: Debug + Send + Sync {
    fn save_provider_config(
        &self,
        input: DesktopProviderConfigInput,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError>;
    fn load_provider_config_summary(
        &self,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError>;
    #[allow(dead_code)]
    fn load_provider_env(&self) -> Result<ProviderEnv, SecretStoreError>;
    fn clear_provider_config(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError>;
}

pub type SharedProviderSecretStore = Arc<dyn ProviderSecretStore>;

#[derive(Clone)]
struct StoredProviderConfig {
    base_url: String,
    api_key: Option<String>,
    model: String,
    timeout_secs: u64,
}

impl Debug for StoredProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredProviderConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &redacted_secret_presence(self.api_key.is_some()))
            .field("model", &self.model)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

impl Default for StoredProviderConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            api_key: None,
            model: DEFAULT_OPENAI_MODEL.to_string(),
            timeout_secs: DEFAULT_OPENAI_TIMEOUT_SECS,
        }
    }
}

impl StoredProviderConfig {
    fn summary(&self) -> DesktopProviderConfigSummary {
        DesktopProviderConfigSummary {
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            timeout_secs: self.timeout_secs,
            has_api_key: self.api_key.is_some(),
        }
    }

    #[allow(dead_code)]
    fn env(&self) -> ProviderEnv {
        ProviderEnv {
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            timeout_secs: self.timeout_secs,
        }
    }
}

#[derive(Default)]
pub struct InMemoryProviderSecretStore {
    config: Mutex<StoredProviderConfig>,
}

impl Debug for InMemoryProviderSecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.config.lock() {
            Ok(config) => f
                .debug_struct("InMemoryProviderSecretStore")
                .field("config", &*config)
                .finish(),
            Err(_) => f
                .debug_struct("InMemoryProviderSecretStore")
                .field("config", &"<poisoned>")
                .finish(),
        }
    }
}

impl InMemoryProviderSecretStore {
    #[allow(dead_code)]
    pub fn shared(self) -> SharedProviderSecretStore {
        Arc::new(self)
    }
}

impl ProviderSecretStore for InMemoryProviderSecretStore {
    fn save_provider_config(
        &self,
        input: DesktopProviderConfigInput,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        let mut config = self.config.lock().map_err(|_| SecretStoreError::Poisoned)?;
        config.base_url = non_blank_or_default(input.base_url, DEFAULT_OPENAI_BASE_URL);
        config.api_key = trimmed_optional(input.api_key);
        config.model = non_blank_or_default(input.model, DEFAULT_OPENAI_MODEL);
        config.timeout_secs = if input.timeout_secs == 0 {
            DEFAULT_OPENAI_TIMEOUT_SECS
        } else {
            input.timeout_secs
        };
        Ok(config.summary())
    }

    fn load_provider_config_summary(
        &self,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        Ok(self
            .config
            .lock()
            .map_err(|_| SecretStoreError::Poisoned)?
            .summary())
    }

    fn load_provider_env(&self) -> Result<ProviderEnv, SecretStoreError> {
        Ok(self
            .config
            .lock()
            .map_err(|_| SecretStoreError::Poisoned)?
            .env())
    }

    fn clear_provider_config(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        let mut config = self.config.lock().map_err(|_| SecretStoreError::Poisoned)?;
        *config = StoredProviderConfig::default();
        Ok(config.summary())
    }
}

pub struct StrongholdReadyProviderSecretStore {
    fallback: InMemoryProviderSecretStore,
}

impl Debug for StrongholdReadyProviderSecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StrongholdReadyProviderSecretStore")
            .field("fallback", &self.fallback)
            .finish()
    }
}

impl StrongholdReadyProviderSecretStore {
    pub fn new() -> Self {
        Self {
            fallback: InMemoryProviderSecretStore::default(),
        }
    }

    pub fn shared(self) -> SharedProviderSecretStore {
        Arc::new(self)
    }
}

impl ProviderSecretStore for StrongholdReadyProviderSecretStore {
    fn save_provider_config(
        &self,
        input: DesktopProviderConfigInput,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        self.fallback.save_provider_config(input)
    }

    fn load_provider_config_summary(
        &self,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        self.fallback.load_provider_config_summary()
    }

    fn load_provider_env(&self) -> Result<ProviderEnv, SecretStoreError> {
        self.fallback.load_provider_env()
    }

    fn clear_provider_config(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        self.fallback.clear_provider_config()
    }
}

fn redacted_secret_presence(has_secret: bool) -> &'static str {
    if has_secret {
        "<redacted>"
    } else {
        "<missing>"
    }
}

fn non_blank_or_default(value: String, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn trimmed_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_provider_config_input_debug_redacts_api_key() {
        let input = DesktopProviderConfigInput {
            base_url: "https://provider.example/v1".to_string(),
            api_key: "input-secret-key".to_string(),
            model: "gpt-test".to_string(),
            timeout_secs: 45,
        };

        let debug = format!("{input:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("input-secret-key"));
    }

    #[test]
    fn in_memory_store_saves_summary_without_exposing_api_key() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: " https://provider.example/v1 ".to_string(),
                api_key: " secret-key ".to_string(),
                model: " gpt-test ".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let summary = store.load_provider_config_summary().expect("summary");
        assert_eq!(summary.base_url, "https://provider.example/v1");
        assert_eq!(summary.model, "gpt-test");
        assert_eq!(summary.timeout_secs, 45);
        assert!(summary.has_api_key);
        assert!(!format!("{summary:?}").contains("secret-key"));
    }

    #[test]
    fn in_memory_store_debug_redacts_api_key() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "in-memory-secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let debug = format!("{store:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("in-memory-secret-key"));
    }

    #[test]
    fn stronghold_ready_store_debug_redacts_fallback_api_key() {
        let store = StrongholdReadyProviderSecretStore::new();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "stronghold-secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let debug = format!("{store:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("stronghold-secret-key"));
    }

    #[test]
    fn app_state_debug_redacts_provider_store_api_key() {
        let state = crate::state::AppState::new();
        state
            .provider_store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "app-state-secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let debug = format!("{state:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("app-state-secret-key"));
    }

    #[test]
    fn in_memory_store_loads_provider_env_for_daemon_without_logging_key() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let env = store.load_provider_env().expect("provider env");
        assert_eq!(env.api_key.as_deref(), Some("secret-key"));
        assert!(!env.redacted_debug().contains("secret-key"));
    }

    #[test]
    fn clear_provider_config_removes_key_and_restores_defaults() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let summary = store.clear_provider_config().expect("clear config");
        assert_eq!(summary.base_url, DEFAULT_OPENAI_BASE_URL);
        assert_eq!(summary.model, DEFAULT_OPENAI_MODEL);
        assert_eq!(summary.timeout_secs, DEFAULT_OPENAI_TIMEOUT_SECS);
        assert!(!summary.has_api_key);
        assert!(store.load_provider_env().expect("env").api_key.is_none());
    }
}
