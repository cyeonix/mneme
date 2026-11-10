use crate::types::{
    CreateRunRequest, CreateRunResponse, EventsResponse, HealthResponse, LifecycleTextRequest,
    RunLifecycleResponse, RunsResponse, ToolCallRequest, ToolCallResponse, ToolLifecycleRequest,
    ToolLifecycleResponse, ToolResultRequest,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use mneme_kernel::{
    kernel_health, ComposedRunService, InMemoryOperationLedger, InMemoryRunProjectionStore, RunId,
    RunServiceError, SharedOperationLedger, SharedRunProjectionStore, SharedRunService, ToolCallId,
};

#[derive(Clone)]
pub struct ApiState {
    pub run_service: SharedRunService,
}

/// 创建第一版本地 API 路由。
/// 这里显式传入 ledger，便于测试中复用同一个内存账本检查副作用。
pub fn build_router(ledger: InMemoryOperationLedger) -> Router {
    build_router_with_ledger(ledger.shared())
}

pub fn build_router_with_ledger(ledger: SharedOperationLedger) -> Router {
    build_router_with_state(ledger, InMemoryRunProjectionStore::new().shared())
}

pub fn build_router_with_state(
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
) -> Router {
    build_router_with_service(ComposedRunService::new(ledger, run_store).shared())
}

pub fn build_router_with_service(run_service: SharedRunService) -> Router {
    Router::new()
        .route("/api/v1/health", get(get_health))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .route(
            "/api/v1/runs/:run_id/tool-calls",
            axum::routing::post(request_tool_call),
        )
        .route(
            "/api/v1/runs/:run_id/tool-calls/:tool_call_id/approve",
            axum::routing::post(approve_tool_call),
        )
        .route(
            "/api/v1/runs/:run_id/tool-calls/:tool_call_id/deny",
            axum::routing::post(deny_tool_call),
        )
        .route(
            "/api/v1/runs/:run_id/tool-calls/:tool_call_id/result",
            axum::routing::post(record_tool_result),
        )
        .route(
            "/api/v1/runs/:run_id/tool-calls/:tool_call_id/fail",
            axum::routing::post(record_tool_failure),
        )
        .route(
            "/api/v1/runs/:run_id/model-requests",
            axum::routing::post(request_model),
        )
        .route(
            "/api/v1/runs/:run_id/assistant-messages",
            axum::routing::post(record_assistant_message),
        )
        .route(
            "/api/v1/runs/:run_id/complete",
            axum::routing::post(complete_run),
        )
        .route("/api/v1/runs/:run_id/fail", axum::routing::post(fail_run))
        .with_state(ApiState { run_service })
}

async fn get_health() -> Json<HealthResponse> {
    Json(kernel_health().into())
}

async fn list_events(State(state): State<ApiState>) -> Result<Json<EventsResponse>, StatusCode> {
    let events = state
        .run_service
        .list_events()
        .await
        .map_err(service_error_to_status)?;
    Ok(Json(EventsResponse { events }))
}

async fn list_runs(State(state): State<ApiState>) -> Result<Json<RunsResponse>, StatusCode> {
    let runs = state
        .run_service
        .list_runs()
        .await
        .map_err(service_error_to_status)?;

    Ok(Json(RunsResponse { runs }))
}

async fn create_run(
    State(state): State<ApiState>,
    Json(request): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, StatusCode> {
    let outcome = state
        .run_service
        .create_run(request.instruction)
        .await
        .map_err(service_error_to_status)?;

    Ok(Json(CreateRunResponse {
        run_id: outcome.run_id.to_string(),
    }))
}

fn parse_run_id(value: &str) -> Result<RunId, StatusCode> {
    value.parse().map_err(|_| StatusCode::BAD_REQUEST)
}

fn parse_tool_call_id(value: &str) -> Result<ToolCallId, StatusCode> {
    value.parse().map_err(|_| StatusCode::BAD_REQUEST)
}

fn lifecycle_response(outcome: mneme_kernel::RunLifecycleOutcome) -> Json<RunLifecycleResponse> {
    Json(RunLifecycleResponse {
        run_id: outcome.run_id.to_string(),
    })
}

fn tool_lifecycle_response(
    outcome: mneme_kernel::ToolLifecycleOutcome,
) -> Json<ToolLifecycleResponse> {
    Json(ToolLifecycleResponse {
        run_id: outcome.run_id.to_string(),
        tool_call_id: outcome.tool_call_id.to_string(),
    })
}

async fn request_tool_call(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<ToolCallRequest>,
) -> Result<Json<ToolCallResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = state
        .run_service
        .request_tool_call(
            run_id,
            request.tool_name,
            request.effect,
            request.input_json,
        )
        .await
        .map_err(service_error_to_status)?;
    Ok(Json(ToolCallResponse {
        run_id: outcome.run_id.to_string(),
        tool_call_id: outcome.tool_call_id.to_string(),
        approval: outcome.approval,
    }))
}

async fn approve_tool_call(
    State(state): State<ApiState>,
    Path((run_id, tool_call_id)): Path<(String, String)>,
    Json(request): Json<ToolLifecycleRequest>,
) -> Result<Json<ToolLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let tool_call_id = parse_tool_call_id(&tool_call_id)?;
    let outcome = state
        .run_service
        .approve_tool_call(run_id, tool_call_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(tool_lifecycle_response(outcome))
}

async fn deny_tool_call(
    State(state): State<ApiState>,
    Path((run_id, tool_call_id)): Path<(String, String)>,
    Json(request): Json<ToolLifecycleRequest>,
) -> Result<Json<ToolLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let tool_call_id = parse_tool_call_id(&tool_call_id)?;
    let outcome = state
        .run_service
        .deny_tool_call(run_id, tool_call_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(tool_lifecycle_response(outcome))
}

async fn record_tool_result(
    State(state): State<ApiState>,
    Path((run_id, tool_call_id)): Path<(String, String)>,
    Json(request): Json<ToolResultRequest>,
) -> Result<Json<ToolLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let tool_call_id = parse_tool_call_id(&tool_call_id)?;
    let outcome = state
        .run_service
        .record_tool_result(run_id, tool_call_id, request.output_json)
        .await
        .map_err(service_error_to_status)?;
    Ok(tool_lifecycle_response(outcome))
}

async fn record_tool_failure(
    State(state): State<ApiState>,
    Path((run_id, tool_call_id)): Path<(String, String)>,
    Json(request): Json<ToolLifecycleRequest>,
) -> Result<Json<ToolLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let tool_call_id = parse_tool_call_id(&tool_call_id)?;
    let outcome = state
        .run_service
        .record_tool_failure(run_id, tool_call_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(tool_lifecycle_response(outcome))
}

async fn request_model(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<LifecycleTextRequest>,
) -> Result<Json<RunLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = state
        .run_service
        .request_model(run_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(lifecycle_response(outcome))
}

async fn record_assistant_message(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<LifecycleTextRequest>,
) -> Result<Json<RunLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = state
        .run_service
        .record_assistant_message(run_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(lifecycle_response(outcome))
}

async fn complete_run(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<LifecycleTextRequest>,
) -> Result<Json<RunLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = state
        .run_service
        .complete_run(run_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(lifecycle_response(outcome))
}

async fn fail_run(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<LifecycleTextRequest>,
) -> Result<Json<RunLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = state
        .run_service
        .fail_run(run_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(lifecycle_response(outcome))
}

fn service_error_to_status(error: RunServiceError) -> StatusCode {
    match error {
        RunServiceError::InvalidInstruction
        | RunServiceError::InvalidRunId
        | RunServiceError::InvalidLifecycleInput
        | RunServiceError::InvalidToolInput
        | RunServiceError::RunAlreadyTerminal => StatusCode::BAD_REQUEST,
        RunServiceError::Ledger(_) | RunServiceError::Projection(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{header, Method, Request},
    };
    use http_body_util::BodyExt;
    use mneme_kernel::{ComposedRunService, OperationLedger, SqliteRunService, SqliteStore};
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_endpoint_returns_ready_kernel_status() {
        let app = build_router(InMemoryOperationLedger::new());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let payload: HealthResponse = serde_json::from_slice(&body).expect("health response");

        assert_eq!(payload.service, "mneme-kernel");
        assert_eq!(payload.status, "ready");
    }

    #[tokio::test]
    async fn create_run_appends_run_started_event() {
        let ledger = InMemoryOperationLedger::new();
        let app = build_router(ledger.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "instruction": "Build the first Mneme run" }).to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(ledger.list().await.expect("events").len(), 1);
    }

    #[tokio::test]
    async fn create_run_rejects_blank_instruction_through_service() {
        let app = build_router(InMemoryOperationLedger::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "instruction": "   " }).to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_runs_returns_created_run_projection() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.shared(), run_store.shared()).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"展示 runs"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let payload: RunsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload.runs.len(), 1);
        assert_eq!(payload.runs[0].instruction, "展示 runs");
    }

    #[tokio::test]
    async fn api_create_run_uses_sqlite_run_service_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let app = build_router_with_service(SqliteRunService::new(store).shared());

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"SQLite service run"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let events_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(events_response.status(), StatusCode::OK);
        let events_body = events_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let events_payload: EventsResponse = serde_json::from_slice(&events_body).unwrap();
        assert_eq!(events_payload.events.len(), 1);

        let runs_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(runs_response.status(), StatusCode::OK);
        let body = runs_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let payload: RunsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload.runs.len(), 1);
        assert_eq!(payload.runs[0].instruction, "SQLite service run");
    }

    #[tokio::test]
    async fn api_records_read_only_tool_call_as_auto_approved() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"读取文件"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let tool_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/tool-calls", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"tool_name":"read_file","effect":"read_only","input_json":{"path":"README.md"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(tool_response.status(), StatusCode::OK);
        let body = tool_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let tool: ToolCallResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(tool.run_id, created.run_id);
        assert_eq!(
            tool.approval,
            mneme_kernel::ToolApprovalDecision::AutoApproved
        );
        assert_eq!(tool.tool_call_id.len(), 36);
    }

    #[tokio::test]
    async fn api_records_tool_result_after_auto_approved_request() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"读取后汇报"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let tool_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/tool-calls", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"tool_name":"read_file","effect":"read_only","input_json":{"path":"README.md"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tool_response.status(), StatusCode::OK);
        let tool_body = tool_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let tool: ToolCallResponse = serde_json::from_slice(&tool_body).unwrap();

        let result_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/api/v1/runs/{}/tool-calls/{}/result",
                        created.run_id, tool.tool_call_id
                    ))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r##"{"output_json":{"content":"# Mneme","bytes":7}}"##,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(result_response.status(), StatusCode::OK);
        let result_body = result_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let result: ToolLifecycleResponse = serde_json::from_slice(&result_body).unwrap();
        assert_eq!(result.run_id, created.run_id);
        assert_eq!(result.tool_call_id, tool.tool_call_id);
    }

    #[tokio::test]
    async fn api_records_manual_tool_approval() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"审批写入工具"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let tool_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/tool-calls", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"tool_name":"write_file","effect":"local_write","input_json":{"path":"README.md"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tool_response.status(), StatusCode::OK);
        let tool_body = tool_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let tool: ToolCallResponse = serde_json::from_slice(&tool_body).unwrap();
        assert_eq!(
            tool.approval,
            mneme_kernel::ToolApprovalDecision::RequiresApproval
        );

        let approval_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/api/v1/runs/{}/tool-calls/{}/approve",
                        created.run_id, tool.tool_call_id
                    ))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text":"human-review"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(approval_response.status(), StatusCode::OK);
        let approval_body = approval_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let approval: ToolLifecycleResponse = serde_json::from_slice(&approval_body).unwrap();
        assert_eq!(approval.run_id, created.run_id);
        assert_eq!(approval.tool_call_id, tool.tool_call_id);
    }

    #[tokio::test]
    async fn api_records_tool_denial() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"拒绝网络工具"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let tool_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/tool-calls", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"tool_name":"fetch_url","effect":"network","input_json":{"url":"https://example.test"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tool_response.status(), StatusCode::OK);
        let tool_body = tool_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let tool: ToolCallResponse = serde_json::from_slice(&tool_body).unwrap();

        let denial_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/api/v1/runs/{}/tool-calls/{}/deny",
                        created.run_id, tool.tool_call_id
                    ))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text":"not needed"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(denial_response.status(), StatusCode::OK);
        let denial_body = denial_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let denial: ToolLifecycleResponse = serde_json::from_slice(&denial_body).unwrap();
        assert_eq!(denial.run_id, created.run_id);
        assert_eq!(denial.tool_call_id, tool.tool_call_id);
    }

    #[tokio::test]
    async fn api_records_tool_failure() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"记录工具失败"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let tool_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/tool-calls", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"tool_name":"read_file","effect":"read_only","input_json":{"path":"missing.md"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tool_response.status(), StatusCode::OK);
        let tool_body = tool_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let tool: ToolCallResponse = serde_json::from_slice(&tool_body).unwrap();

        let failure_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/api/v1/runs/{}/tool-calls/{}/fail",
                        created.run_id, tool.tool_call_id
                    ))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text":"file not found"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(failure_response.status(), StatusCode::OK);
        let failure_body = failure_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let failure: ToolLifecycleResponse = serde_json::from_slice(&failure_body).unwrap();
        assert_eq!(failure.run_id, created.run_id);
        assert_eq!(failure.tool_call_id, tool.tool_call_id);
    }

    #[tokio::test]
    async fn api_rejects_tool_request_after_completed_run() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"终止后拒绝工具"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let complete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/complete", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text":"已经完成"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(complete_response.status(), StatusCode::OK);

        let tool_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/tool-calls", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"tool_name":"read_file","effect":"read_only","input_json":{"path":"README.md"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(tool_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_rejects_invalid_tool_call_id() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store).shared();
        let app = build_router_with_service(service);

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"校验工具 ID"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let result_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/api/v1/runs/{}/tool-calls/not-a-uuid/result",
                        created.run_id
                    ))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"output_json":{"ok":true}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(result_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_rejects_blank_lifecycle_text() {
        let app = build_router(InMemoryOperationLedger::new());
        let run_id = mneme_kernel::RunId::new();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{run_id}/model-requests"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text":"   "}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_lifecycle_rejects_model_request_after_run_is_completed() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let app = build_router_with_service(SqliteRunService::new(store).shared());

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"端到端生命周期"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        let complete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/complete", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text":"总结完成"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(complete_response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/runs/{}/model-requests", created.run_id))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"text":"不应恢复运行"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_records_success_lifecycle_through_service() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let app = build_router_with_service(SqliteRunService::new(store).shared());

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"instruction":"端到端生命周期"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_body = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

        for (path, text) in [
            (
                format!("/api/v1/runs/{}/model-requests", created.run_id),
                "请总结今天",
            ),
            (
                format!("/api/v1/runs/{}/assistant-messages", created.run_id),
                "今天完成了 M2。",
            ),
            (
                format!("/api/v1/runs/{}/complete", created.run_id),
                "总结完成",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(path)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(json!({ "text": text }).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let runs_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = runs_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let runs: RunsResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(runs.runs[0].run_id.to_string(), created.run_id);
        assert_eq!(runs.runs[0].status, mneme_kernel::RunStatus::Completed);
        assert_eq!(runs.runs[0].summary.as_deref(), Some("总结完成"));
    }
}
