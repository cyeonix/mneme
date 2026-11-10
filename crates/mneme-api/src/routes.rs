use crate::types::{
    CreateRunRequest, CreateRunResponse, EventsResponse, HealthResponse, RunsResponse,
};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use mneme_kernel::{
    kernel_health, ComposedRunService, InMemoryOperationLedger, InMemoryRunProjectionStore,
    RunServiceError, SharedOperationLedger, SharedRunProjectionStore, SharedRunService,
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

fn service_error_to_status(error: RunServiceError) -> StatusCode {
    match error {
        RunServiceError::InvalidInstruction => StatusCode::BAD_REQUEST,
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
}
