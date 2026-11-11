use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::{
    OperationEvent, OperationEventKind, RunId, RunServiceError, SharedOperationLedger,
    SharedRunService, WorkerId, WorkerJobId,
};

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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkerRuntimeError {
    #[error("worker job projection failed: {0}")]
    Projection(String),
    #[error("worker job payload is invalid: {0}")]
    InvalidPayload(String),
}

#[async_trait]
pub trait WorkerJobProjectionStore: Send + Sync {
    async fn apply(&self, event: &OperationEvent) -> Result<(), WorkerRuntimeError>;
    async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeError>;
    async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), WorkerRuntimeError>;
}

pub type SharedWorkerJobProjectionStore = Arc<dyn WorkerJobProjectionStore>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunModelWithWorkerOutcome {
    pub run_id: RunId,
    pub worker_job_id: WorkerJobId,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkerRuntimeServiceError {
    #[error("worker runtime input is invalid")]
    InvalidInput,
    #[error("run is unknown")]
    UnknownRun,
    #[error("run is already terminal")]
    RunAlreadyTerminal,
    #[error("provider worker is missing required configuration")]
    ProviderMissingConfiguration,
    #[error("provider gateway failed: {0}")]
    ProviderBadGateway(String),
    #[error("provider request timed out")]
    ProviderTimeout,
    #[error("worker runtime ledger failed: {0}")]
    Ledger(String),
    #[error("worker runtime projection failed: {0}")]
    Projection(String),
    #[error("run service failed: {0}")]
    RunService(String),
}

#[async_trait]
pub trait WorkerRuntimeService: Send + Sync {
    async fn run_model_completion(
        &self,
        run_id: RunId,
        prompt: String,
    ) -> Result<RunModelWithWorkerOutcome, WorkerRuntimeServiceError>;

    async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeServiceError>;
}

pub type SharedWorkerRuntimeService = Arc<dyn WorkerRuntimeService>;

#[derive(Clone)]
pub struct ComposedWorkerRuntimeService {
    worker_id: WorkerId,
    model: String,
    ledger: SharedOperationLedger,
    run_service: SharedRunService,
    worker_jobs: SharedWorkerJobProjectionStore,
    provider: SharedProviderWorker,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryWorkerJobProjectionStore {
    jobs: Arc<RwLock<Vec<WorkerJobRecord>>>,
}

impl ComposedWorkerRuntimeService {
    pub fn new(
        worker_id: WorkerId,
        model: String,
        ledger: SharedOperationLedger,
        run_service: SharedRunService,
        worker_jobs: SharedWorkerJobProjectionStore,
        provider: SharedProviderWorker,
    ) -> Self {
        Self {
            worker_id,
            model,
            ledger,
            run_service,
            worker_jobs,
            provider,
        }
    }

    pub fn shared(self) -> SharedWorkerRuntimeService {
        Arc::new(self)
    }

    async fn append_and_apply_worker_event(
        &self,
        event: OperationEvent,
    ) -> Result<(), WorkerRuntimeServiceError> {
        self.ledger
            .append(event.clone())
            .await
            .map_err(|error| WorkerRuntimeServiceError::Ledger(error.to_string()))?;
        self.worker_jobs
            .apply(&event)
            .await
            .map_err(|error| WorkerRuntimeServiceError::Projection(error.to_string()))
    }
}

#[async_trait]
impl WorkerRuntimeService for ComposedWorkerRuntimeService {
    async fn run_model_completion(
        &self,
        run_id: RunId,
        prompt: String,
    ) -> Result<RunModelWithWorkerOutcome, WorkerRuntimeServiceError> {
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(WorkerRuntimeServiceError::InvalidInput);
        }

        let worker_job_id = WorkerJobId::new();
        let worker_id = self.worker_id;
        let kind = WorkerJobKind::ModelCompletion;
        let started = OperationEvent::new_worker_job_started(
            run_id,
            worker_job_id,
            worker_id,
            kind,
            Some(self.model.clone()),
        );
        self.append_and_apply_worker_event(started).await?;

        self.run_service
            .request_model(run_id, prompt.clone())
            .await
            .map_err(map_run_service_error)?;

        let response = match self
            .provider
            .complete(ModelCompletionRequest {
                run_id,
                prompt,
                model: self.model.clone(),
            })
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let runtime_error = map_provider_worker_error(error);
                let reason = provider_failure_reason(&runtime_error);
                let failed = OperationEvent::new_worker_job_failed(
                    run_id,
                    worker_job_id,
                    worker_id,
                    kind,
                    reason.clone(),
                );
                self.append_and_apply_worker_event(failed).await?;
                self.run_service
                    .fail_run(run_id, reason)
                    .await
                    .map_err(map_run_service_error)?;
                return Err(runtime_error);
            }
        };

        let completed =
            OperationEvent::new_worker_job_completed(run_id, worker_job_id, worker_id, kind);
        self.append_and_apply_worker_event(completed).await?;
        self.run_service
            .record_assistant_message(run_id, response.content.clone())
            .await
            .map_err(map_run_service_error)?;
        self.run_service
            .complete_run(run_id, response.content)
            .await
            .map_err(map_run_service_error)?;

        Ok(RunModelWithWorkerOutcome {
            run_id,
            worker_job_id,
        })
    }

    async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeServiceError> {
        self.worker_jobs
            .list_worker_jobs()
            .await
            .map_err(|error| WorkerRuntimeServiceError::Projection(error.to_string()))
    }
}

fn map_run_service_error(error: RunServiceError) -> WorkerRuntimeServiceError {
    match error {
        RunServiceError::InvalidLifecycleInput => WorkerRuntimeServiceError::InvalidInput,
        RunServiceError::InvalidRunId => WorkerRuntimeServiceError::UnknownRun,
        RunServiceError::RunAlreadyTerminal => WorkerRuntimeServiceError::RunAlreadyTerminal,
        RunServiceError::Ledger(message) => WorkerRuntimeServiceError::Ledger(message),
        RunServiceError::Projection(message) => WorkerRuntimeServiceError::Projection(message),
        other => WorkerRuntimeServiceError::RunService(other.to_string()),
    }
}

fn map_provider_worker_error(error: ProviderWorkerError) -> WorkerRuntimeServiceError {
    match error {
        ProviderWorkerError::MissingConfiguration => {
            WorkerRuntimeServiceError::ProviderMissingConfiguration
        }
        ProviderWorkerError::Timeout => WorkerRuntimeServiceError::ProviderTimeout,
        ProviderWorkerError::ProviderStatus { status, .. } => {
            WorkerRuntimeServiceError::ProviderBadGateway(format!(
                "provider returned status {status}"
            ))
        }
        ProviderWorkerError::InvalidRequest => WorkerRuntimeServiceError::ProviderBadGateway(
            "provider request was rejected".to_string(),
        ),
        ProviderWorkerError::Http(_) => WorkerRuntimeServiceError::ProviderBadGateway(
            "provider HTTP request failed".to_string(),
        ),
        ProviderWorkerError::MalformedResponse(_) => WorkerRuntimeServiceError::ProviderBadGateway(
            "provider returned malformed response".to_string(),
        ),
    }
}

fn provider_failure_reason(error: &WorkerRuntimeServiceError) -> String {
    match error {
        WorkerRuntimeServiceError::ProviderMissingConfiguration => {
            "provider worker is missing required configuration".to_string()
        }
        WorkerRuntimeServiceError::ProviderTimeout => "provider request timed out".to_string(),
        WorkerRuntimeServiceError::ProviderBadGateway(message) => message.clone(),
        _ => "provider worker failed".to_string(),
    }
}

impl InMemoryWorkerJobProjectionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedWorkerJobProjectionStore {
        Arc::new(self)
    }
}

#[async_trait]
impl WorkerJobProjectionStore for InMemoryWorkerJobProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), WorkerRuntimeError> {
        let mut jobs = self.jobs.write().await;
        apply_worker_job_event_to_records(&mut jobs, event);
        Ok(())
    }

    async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeError> {
        Ok(self.jobs.read().await.clone())
    }

    async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), WorkerRuntimeError> {
        let mut jobs = self.jobs.write().await;
        jobs.clear();
        for event in events {
            apply_worker_job_event_to_records(&mut jobs, event);
        }
        Ok(())
    }
}

fn apply_worker_job_event_to_records(jobs: &mut Vec<WorkerJobRecord>, event: &OperationEvent) {
    match &event.kind {
        OperationEventKind::WorkerJobStarted {
            worker_job_id,
            worker_id,
            kind,
            model,
        } => {
            let record = WorkerJobRecord {
                worker_job_id: *worker_job_id,
                worker_id: *worker_id,
                run_id: event.run_id,
                kind: *kind,
                status: WorkerJobStatus::Started,
                model: model.clone(),
                error: None,
                started_at: event.occurred_at,
                completed_at: None,
            };
            upsert_worker_job_record(jobs, record);
        }
        OperationEventKind::WorkerJobCompleted {
            worker_job_id,
            worker_id,
            kind,
        } => {
            let record = terminal_worker_job_record(
                jobs,
                *worker_job_id,
                *worker_id,
                event.run_id,
                *kind,
                WorkerJobStatus::Completed,
                None,
                event.occurred_at,
            );
            upsert_worker_job_record(jobs, record);
        }
        OperationEventKind::WorkerJobFailed {
            worker_job_id,
            worker_id,
            kind,
            reason,
        } => {
            let record = terminal_worker_job_record(
                jobs,
                *worker_job_id,
                *worker_id,
                event.run_id,
                *kind,
                WorkerJobStatus::Failed,
                Some(reason.clone()),
                event.occurred_at,
            );
            upsert_worker_job_record(jobs, record);
        }
        _ => {}
    }
    jobs.sort_by(|left, right| right.started_at.cmp(&left.started_at));
}

#[allow(clippy::too_many_arguments)]
fn terminal_worker_job_record(
    jobs: &[WorkerJobRecord],
    worker_job_id: WorkerJobId,
    worker_id: WorkerId,
    run_id: RunId,
    kind: WorkerJobKind,
    status: WorkerJobStatus,
    error: Option<String>,
    occurred_at: OffsetDateTime,
) -> WorkerJobRecord {
    if let Some(existing) = jobs.iter().find(|job| job.worker_job_id == worker_job_id) {
        let mut record = existing.clone();
        record.worker_id = worker_id;
        record.run_id = run_id;
        record.kind = kind;
        record.status = status;
        record.error = error;
        record.completed_at = Some(occurred_at);
        record
    } else {
        WorkerJobRecord {
            worker_job_id,
            worker_id,
            run_id,
            kind,
            status,
            model: None,
            error,
            started_at: occurred_at,
            completed_at: Some(occurred_at),
        }
    }
}

fn upsert_worker_job_record(jobs: &mut Vec<WorkerJobRecord>, record: WorkerJobRecord) {
    if let Some(existing) = jobs
        .iter_mut()
        .find(|job| job.worker_job_id == record.worker_job_id)
    {
        *existing = record;
    } else {
        jobs.push(record);
    }
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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
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
    response: Result<ModelCompletionResponse, ProviderWorkerError>,
}

impl MockProviderWorker {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            worker_id: WorkerId::new(),
            response: Ok(ModelCompletionResponse {
                content: content.into(),
                usage: None,
            }),
        }
    }

    pub fn failing(error: ProviderWorkerError) -> Self {
        Self {
            worker_id: WorkerId::new(),
            response: Err(error),
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
        self.response.clone()
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
    use crate::{
        ComposedRunService, InMemoryOperationLedger, InMemoryRunProjectionStore, OperationEvent,
        OperationLedger, RunId, RunService, RunStatus, WorkerId, WorkerJobId,
    };
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
    async fn in_memory_worker_job_projection_tracks_started_completed_failed() {
        let store = InMemoryWorkerJobProjectionStore::new();
        let run_id = RunId::new();
        let worker_id = WorkerId::new();
        let job_id = WorkerJobId::new();

        let started = OperationEvent::new_worker_job_started(
            run_id,
            job_id,
            worker_id,
            WorkerJobKind::ModelCompletion,
            Some("gpt-test".to_string()),
        );
        store.apply(&started).await.unwrap();
        assert_eq!(
            store.list_worker_jobs().await.unwrap()[0].status,
            WorkerJobStatus::Started
        );

        let completed = OperationEvent::new_worker_job_completed(
            run_id,
            job_id,
            worker_id,
            WorkerJobKind::ModelCompletion,
        );
        store.apply(&completed).await.unwrap();
        let job = &store.list_worker_jobs().await.unwrap()[0];
        assert_eq!(job.status, WorkerJobStatus::Completed);
        assert!(job.completed_at.is_some());

        let failed_job_id = WorkerJobId::new();
        let failed = OperationEvent::new_worker_job_failed(
            run_id,
            failed_job_id,
            worker_id,
            WorkerJobKind::ModelCompletion,
            "provider returned 500",
        );
        store.apply(&failed).await.unwrap();
        let jobs = store.list_worker_jobs().await.unwrap();
        let failed_job = jobs
            .iter()
            .find(|job| job.worker_job_id == failed_job_id)
            .unwrap();
        assert_eq!(failed_job.status, WorkerJobStatus::Failed);
        assert_eq!(failed_job.error.as_deref(), Some("provider returned 500"));
        assert!(failed_job.completed_at.is_some());
    }

    #[tokio::test]
    async fn composed_worker_runtime_success_drives_run_lifecycle() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let run_service =
            ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());
        let worker_jobs = InMemoryWorkerJobProjectionStore::new();
        let provider = MockProviderWorker::new("assistant reply").shared();
        let runtime = ComposedWorkerRuntimeService::new(
            WorkerId::new(),
            "mock-model".to_string(),
            ledger.clone().shared(),
            run_service.clone().shared(),
            worker_jobs.clone().shared(),
            provider,
        );
        let created = ComposedRunService::new(ledger.clone().shared(), run_store.shared())
            .create_run("write reply".to_string())
            .await
            .unwrap();

        let outcome = runtime
            .run_model_completion(created.run_id, "hello model".to_string())
            .await
            .unwrap();

        assert_eq!(outcome.run_id, created.run_id);
        let jobs = runtime.list_worker_jobs().await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].worker_job_id, outcome.worker_job_id);
        assert_eq!(jobs[0].status, WorkerJobStatus::Completed);
        assert_eq!(jobs[0].model.as_deref(), Some("mock-model"));
        assert!(jobs[0].completed_at.is_some());
        let runs = run_service.list_runs().await.unwrap();
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].summary.as_deref(), Some("assistant reply"));
        let events = ledger.list().await.unwrap();
        assert_eq!(events.len(), 6);
    }

    #[tokio::test]
    async fn composed_worker_runtime_provider_failure_fails_run() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let run_service =
            ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());
        let worker_jobs = InMemoryWorkerJobProjectionStore::new();
        let provider = MockProviderWorker::failing(ProviderWorkerError::ProviderStatus {
            status: 500,
            body: "provider failed".to_string(),
        })
        .shared();
        let runtime = ComposedWorkerRuntimeService::new(
            WorkerId::new(),
            "mock-model".to_string(),
            ledger.clone().shared(),
            run_service.clone().shared(),
            worker_jobs.clone().shared(),
            provider,
        );
        let created = ComposedRunService::new(ledger.clone().shared(), run_store.shared())
            .create_run("write reply".to_string())
            .await
            .unwrap();

        let error = runtime
            .run_model_completion(created.run_id, "hello model".to_string())
            .await
            .unwrap_err();

        assert_eq!(
            error,
            WorkerRuntimeServiceError::ProviderBadGateway(
                "provider returned status 500".to_string()
            )
        );
        let jobs = runtime.list_worker_jobs().await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, WorkerJobStatus::Failed);
        assert_eq!(
            jobs[0].error.as_deref(),
            Some("provider returned status 500")
        );
        let runs = run_service.list_runs().await.unwrap();
        assert_eq!(runs[0].status, RunStatus::Failed);
        assert_eq!(
            runs[0].error.as_deref(),
            Some("provider returned status 500")
        );
        assert!(!format!("{error}").contains("provider failed"));
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
