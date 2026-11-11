use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;

use crate::{RunId, WorkerId, WorkerJobId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerJobKind {
    ModelCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerJobStatus {
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerJobRecord {
    pub worker_job_id: WorkerJobId,
    pub worker_id: WorkerId,
    pub run_id: RunId,
    pub kind: WorkerJobKind,
    pub status: WorkerJobStatus,
    pub model: Option<String>,
    pub error: Option<String>,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCompletionRequest {
    pub run_id: RunId,
    pub prompt: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCompletionResponse {
    pub content: String,
    pub usage: Option<ModelUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProviderWorkerError {
    #[error("provider worker is missing required configuration")]
    MissingConfiguration,
    #[error("provider worker request is invalid")]
    InvalidRequest,
    #[error("provider HTTP request failed: {0}")]
    Http(String),
    #[error("provider returned status {status}: {body}")]
    ProviderStatus { status: u16, body: String },
    #[error("provider response is malformed: {0}")]
    MalformedResponse(String),
    #[error("provider request timed out")]
    Timeout,
}

#[async_trait]
pub trait ProviderWorker: Send + Sync {
    fn worker_id(&self) -> WorkerId;

    async fn complete(
        &self,
        request: ModelCompletionRequest,
    ) -> Result<ModelCompletionResponse, ProviderWorkerError>;
}

pub type SharedProviderWorker = Arc<dyn ProviderWorker>;

#[derive(Debug, Clone)]
pub struct MockProviderWorker {
    worker_id: WorkerId,
    content: String,
}

impl MockProviderWorker {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            worker_id: WorkerId::new(),
            content: content.into(),
        }
    }

    pub fn shared(self) -> SharedProviderWorker {
        Arc::new(self)
    }
}

#[async_trait]
impl ProviderWorker for MockProviderWorker {
    fn worker_id(&self) -> WorkerId {
        self.worker_id
    }

    async fn complete(
        &self,
        _request: ModelCompletionRequest,
    ) -> Result<ModelCompletionResponse, ProviderWorkerError> {
        Ok(ModelCompletionResponse {
            content: self.content.clone(),
            usage: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiProviderConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiHttpRequest {
    pub url: String,
    pub bearer_token: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiHttpResponse {
    pub status: u16,
    pub body: String,
}

#[async_trait]
pub trait OpenAiHttpClient: Send + Sync {
    async fn post_chat_completions(
        &self,
        request: OpenAiHttpRequest,
        timeout_secs: u64,
    ) -> Result<OpenAiHttpResponse, ProviderWorkerError>;
}

pub type SharedOpenAiHttpClient = Arc<dyn OpenAiHttpClient>;

#[derive(Debug, Clone, Default)]
pub struct ReqwestOpenAiHttpClient {
    client: reqwest::Client,
}

impl ReqwestOpenAiHttpClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedOpenAiHttpClient {
        Arc::new(self)
    }
}

#[async_trait]
impl OpenAiHttpClient for ReqwestOpenAiHttpClient {
    async fn post_chat_completions(
        &self,
        request: OpenAiHttpRequest,
        timeout_secs: u64,
    ) -> Result<OpenAiHttpResponse, ProviderWorkerError> {
        let response = self
            .client
            .post(&request.url)
            .bearer_auth(request.bearer_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request.body)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(map_reqwest_error)?;

        Ok(OpenAiHttpResponse { status, body })
    }
}

fn map_reqwest_error(error: reqwest::Error) -> ProviderWorkerError {
    if error.is_timeout() {
        ProviderWorkerError::Timeout
    } else {
        ProviderWorkerError::Http(error.to_string())
    }
}

#[derive(Clone)]
pub struct OpenAiCompatibleProviderWorker {
    worker_id: WorkerId,
    config: OpenAiProviderConfig,
    http: SharedOpenAiHttpClient,
}

impl OpenAiCompatibleProviderWorker {
    pub fn new(
        worker_id: WorkerId,
        config: OpenAiProviderConfig,
        http: SharedOpenAiHttpClient,
    ) -> Self {
        Self {
            worker_id,
            config,
            http,
        }
    }
}

#[async_trait]
impl ProviderWorker for OpenAiCompatibleProviderWorker {
    fn worker_id(&self) -> WorkerId {
        self.worker_id
    }

    async fn complete(
        &self,
        request: ModelCompletionRequest,
    ) -> Result<ModelCompletionResponse, ProviderWorkerError> {
        if request.prompt.trim().is_empty() || request.model.trim().is_empty() {
            return Err(ProviderWorkerError::InvalidRequest);
        }

        let api_key = self
            .config
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or(ProviderWorkerError::MissingConfiguration)?;
        let base_url = self.config.base_url.trim_end_matches('/');
        if base_url.is_empty() {
            return Err(ProviderWorkerError::MissingConfiguration);
        }

        let body = serde_json::json!({
            "model": request.model,
            "messages": [{ "role": "user", "content": request.prompt }],
            "stream": false,
        })
        .to_string();

        let response = self
            .http
            .post_chat_completions(
                OpenAiHttpRequest {
                    url: format!("{base_url}/chat/completions"),
                    bearer_token: api_key.to_string(),
                    body,
                },
                self.config.timeout_secs,
            )
            .await?;

        if !(200..300).contains(&response.status) {
            return Err(ProviderWorkerError::ProviderStatus {
                status: response.status,
                body: response.body,
            });
        }

        parse_openai_response(&response.body)
    }
}

fn parse_openai_response(body: &str) -> Result<ModelCompletionResponse, ProviderWorkerError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| ProviderWorkerError::MalformedResponse(error.to_string()))?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| {
            ProviderWorkerError::MalformedResponse("missing choices[0].message.content".to_string())
        })?
        .to_string();
    let usage = value.get("usage").and_then(|usage| {
        if usage.is_object() {
            Some(ModelUsage {
                prompt_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
                completion_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
                total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
            })
        } else {
            None
        }
    });

    Ok(ModelCompletionResponse { content, usage })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RunId, WorkerId, WorkerJobId};
    use std::sync::{Arc, Mutex};

    #[test]
    fn worker_ids_are_displayable_and_parseable() {
        let worker_id = WorkerId::new();
        assert_eq!(
            worker_id.to_string().parse::<WorkerId>().unwrap(),
            worker_id
        );

        let job_id = WorkerJobId::new();
        assert_eq!(job_id.to_string().parse::<WorkerJobId>().unwrap(), job_id);
    }

    #[tokio::test]
    async fn mock_provider_returns_configured_completion() {
        let worker = MockProviderWorker::new("assistant reply");
        let response = worker
            .complete(ModelCompletionRequest {
                run_id: RunId::new(),
                prompt: "hello".to_string(),
                model: "mock-model".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(response.content, "assistant reply");
    }

    #[tokio::test]
    async fn openai_provider_maps_chat_completion_response() {
        let http = MockOpenAiHttpClient::new(
            200,
            r#"{"choices":[{"message":{"role":"assistant","content":"mapped reply"}}],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}"#,
        );
        let provider = OpenAiCompatibleProviderWorker::new(
            WorkerId::new(),
            OpenAiProviderConfig {
                base_url: "https://example.test/v1".to_string(),
                api_key: Some("secret-value".to_string()),
                model: "gpt-test".to_string(),
                timeout_secs: 30,
            },
            http.shared(),
        );

        let response = provider
            .complete(ModelCompletionRequest {
                run_id: RunId::new(),
                prompt: "hello".to_string(),
                model: "gpt-test".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(response.content, "mapped reply");
        assert_eq!(response.usage.unwrap().total_tokens, Some(7));
        assert_eq!(
            http.last_request().unwrap().url,
            "https://example.test/v1/chat/completions"
        );
        assert!(http
            .last_request()
            .unwrap()
            .body
            .contains("\"stream\":false"));
        assert!(http
            .last_request()
            .unwrap()
            .body
            .contains("\"content\":\"hello\""));
    }

    #[tokio::test]
    async fn openai_provider_requires_api_key_without_leaking_it() {
        let provider = OpenAiCompatibleProviderWorker::new(
            WorkerId::new(),
            OpenAiProviderConfig {
                base_url: "https://example.test/v1".to_string(),
                api_key: None,
                model: "gpt-test".to_string(),
                timeout_secs: 30,
            },
            MockOpenAiHttpClient::new(200, "{}").shared(),
        );

        let error = provider
            .complete(ModelCompletionRequest {
                run_id: RunId::new(),
                prompt: "hello".to_string(),
                model: "gpt-test".to_string(),
            })
            .await
            .unwrap_err();

        assert_eq!(error, ProviderWorkerError::MissingConfiguration);
        assert!(!error.to_string().contains("secret"));
    }

    #[derive(Debug, Clone)]
    struct MockOpenAiHttpClient {
        status: u16,
        body: String,
        last_request: Arc<Mutex<Option<OpenAiHttpRequest>>>,
    }

    impl MockOpenAiHttpClient {
        fn new(status: u16, body: impl Into<String>) -> Self {
            Self {
                status,
                body: body.into(),
                last_request: Arc::new(Mutex::new(None)),
            }
        }

        fn shared(&self) -> SharedOpenAiHttpClient {
            Arc::new(self.clone())
        }

        fn last_request(&self) -> Option<OpenAiHttpRequest> {
            self.last_request.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl OpenAiHttpClient for MockOpenAiHttpClient {
        async fn post_chat_completions(
            &self,
            request: OpenAiHttpRequest,
            _timeout_secs: u64,
        ) -> Result<OpenAiHttpResponse, ProviderWorkerError> {
            *self.last_request.lock().unwrap() = Some(request);
            Ok(OpenAiHttpResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }
    }
}
