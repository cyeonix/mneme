use serde::{Deserialize, Serialize};
use std::sync::Mutex;

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
pub struct DesktopProviderConfigSummary {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
    pub has_api_key: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopRuntimeStatus {
    pub daemon: DesktopDaemonStatus,
    pub provider: DesktopProviderConfigSummary,
}

#[derive(Debug)]
pub struct AppState {
    pub api_base_url: String,
    pub daemon: Mutex<DesktopDaemonStatus>,
    pub provider: Mutex<DesktopProviderConfigSummary>,
}

impl AppState {
    pub fn new() -> Self {
        let api_base_url = "http://127.0.0.1:3766".to_string();
        Self {
            api_base_url: api_base_url.clone(),
            daemon: Mutex::new(DesktopDaemonStatus {
                state: DaemonProcessState::NotStarted,
                api_base_url: Some(api_base_url),
                last_error: None,
            }),
            provider: Mutex::new(DesktopProviderConfigSummary {
                base_url: "https://api.openai.com/v1".to_string(),
                model: "gpt-4o-mini".to_string(),
                timeout_secs: 30,
                has_api_key: false,
            }),
        }
    }
}
