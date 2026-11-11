use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use mneme_kernel::{
    DeviceId, DeviceRecord, RegisterDeviceResponse as KernelRegisterDeviceResponse,
    SyncCursorRecord, SyncObjectKind, SyncedEventRecord, SyncedObjectRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    service::{
        BackupExportOutcome, PullEventsOutcome, PullObjectsOutcome, PushEventsOutcome,
        PushObjectsOutcome,
    },
    SyncServerError, SyncServerService,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterDeviceRequest {
    pub display_name: String,
    pub public_key_fingerprint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterDeviceResponse {
    pub device: DeviceRecord,
    pub cursor: SyncCursorRecord,
    pub device_token: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthenticatedDeviceRequest {
    pub device_id: DeviceId,
    pub device_token: String,
}

#[derive(Debug, Deserialize)]
pub struct PushEventsRequest {
    pub device_id: DeviceId,
    pub device_token: String,
    pub events: Vec<SyncedEventRecord>,
}

#[derive(Debug, Deserialize)]
pub struct PullEventsRequest {
    pub device_id: DeviceId,
    pub device_token: String,
    pub after_server_sequence: i64,
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct AckEventsRequest {
    pub device_id: DeviceId,
    pub device_token: String,
    pub last_seen_server_sequence: i64,
}

#[derive(Debug, Deserialize)]
pub struct PushObjectsRequest {
    pub device_id: DeviceId,
    pub device_token: String,
    pub objects: Vec<SyncedObjectRecord>,
}

#[derive(Debug, Deserialize)]
pub struct PullObjectsRequest {
    pub device_id: DeviceId,
    pub device_token: String,
    pub object_kind: Option<SyncObjectKind>,
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ImportBackupRequest {
    pub payload_json: serde_json::Value,
}

pub fn build_router(service: SyncServerService) -> Router {
    Router::new()
        .route("/api/v1/server/health", get(get_health))
        .route("/api/v1/server/overview", get(server_overview))
        .route("/api/v1/devices/register", post(register_device))
        .route("/api/v1/devices", get(list_devices))
        .route("/api/v1/sync/cursors", get(list_cursors))
        .route("/api/v1/sync/push-events", post(push_events))
        .route("/api/v1/sync/pull-events", post(pull_events))
        .route("/api/v1/sync/ack-events", post(ack_events))
        .route("/api/v1/sync/push-objects", post(push_objects))
        .route("/api/v1/sync/pull-objects", post(pull_objects))
        .route("/api/v1/backups/export", post(export_backup))
        .route("/api/v1/backups/import", post(import_backup))
        .route("/api/v1/backups", get(list_backups))
        .with_state(service)
}

async fn get_health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "mneme-server".to_string(),
        status: "ready".to_string(),
    })
}

async fn server_overview(
    State(service): State<SyncServerService>,
) -> Result<Json<crate::ServerOverviewResponse>, ApiError> {
    service.server_overview().await.map(Json).map_err(ApiError)
}

async fn register_device(
    State(service): State<SyncServerService>,
    Json(request): Json<RegisterDeviceRequest>,
) -> Result<Json<RegisterDeviceResponse>, ApiError> {
    let registered = service
        .register_device(request.display_name, request.public_key_fingerprint)
        .await
        .map_err(ApiError)?;
    Ok(Json(RegisterDeviceResponse {
        device: registered.device,
        cursor: registered.cursor,
        device_token: registered.device_token,
    }))
}

async fn list_devices(
    State(service): State<SyncServerService>,
) -> Result<Json<Vec<DeviceRecord>>, ApiError> {
    service.list_devices().await.map(Json).map_err(ApiError)
}

async fn list_cursors(
    State(service): State<SyncServerService>,
) -> Result<Json<Vec<SyncCursorRecord>>, ApiError> {
    service.list_cursors().await.map(Json).map_err(ApiError)
}

async fn push_events(
    State(service): State<SyncServerService>,
    Json(request): Json<PushEventsRequest>,
) -> Result<Json<PushEventsOutcome>, ApiError> {
    verify_device_token(&service, request.device_id, &request.device_token).await?;
    service
        .push_events(request.device_id, request.events)
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn pull_events(
    State(service): State<SyncServerService>,
    Json(request): Json<PullEventsRequest>,
) -> Result<Json<PullEventsOutcome>, ApiError> {
    verify_device_token(&service, request.device_id, &request.device_token).await?;
    service
        .pull_events(
            request.device_id,
            request.after_server_sequence,
            request.limit,
        )
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn ack_events(
    State(service): State<SyncServerService>,
    Json(request): Json<AckEventsRequest>,
) -> Result<Json<SyncCursorRecord>, ApiError> {
    verify_device_token(&service, request.device_id, &request.device_token).await?;
    service
        .ack_events(request.device_id, request.last_seen_server_sequence)
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn push_objects(
    State(service): State<SyncServerService>,
    Json(request): Json<PushObjectsRequest>,
) -> Result<Json<PushObjectsOutcome>, ApiError> {
    verify_device_token(&service, request.device_id, &request.device_token).await?;
    service
        .push_objects(request.device_id, request.objects)
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn pull_objects(
    State(service): State<SyncServerService>,
    Json(request): Json<PullObjectsRequest>,
) -> Result<Json<PullObjectsOutcome>, ApiError> {
    verify_device_token(&service, request.device_id, &request.device_token).await?;
    service
        .pull_objects(request.device_id, request.object_kind, request.limit)
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn export_backup(
    State(service): State<SyncServerService>,
) -> Result<Json<BackupExportOutcome>, ApiError> {
    service.export_backup().await.map(Json).map_err(ApiError)
}

async fn import_backup(
    State(service): State<SyncServerService>,
    Json(request): Json<ImportBackupRequest>,
) -> Result<Json<mneme_kernel::BackupRecord>, ApiError> {
    service
        .import_backup(request.payload_json)
        .await
        .map(Json)
        .map_err(ApiError)
}

async fn list_backups(
    State(service): State<SyncServerService>,
) -> Result<Json<Vec<mneme_kernel::BackupRecord>>, ApiError> {
    service.list_backups().await.map(Json).map_err(ApiError)
}

async fn verify_device_token(
    service: &SyncServerService,
    device_id: DeviceId,
    device_token: &str,
) -> Result<(), ApiError> {
    service
        .verify_device_token(device_id, device_token)
        .await
        .map_err(ApiError)
}

struct ApiError(SyncServerError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            SyncServerError::InvalidInput => (StatusCode::BAD_REQUEST, "invalid input"),
            SyncServerError::UnknownDevice => (StatusCode::NOT_FOUND, "unknown device"),
            SyncServerError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            SyncServerError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage error"),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<RegisterDeviceResponse> for KernelRegisterDeviceResponse {
    fn from(value: RegisterDeviceResponse) -> Self {
        Self {
            device: value.device,
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
    use mneme_kernel::{DeviceId, EventId, RunId, SyncedEventRecord};
    use serde_json::json;
    use tower::ServiceExt;

    async fn test_router() -> Router {
        let store = crate::storage::SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        build_router(SyncServerService::new(store))
    }

    fn request(method: Method, uri: &str, payload: Option<serde_json::Value>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if payload.is_some() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        builder
            .body(payload.map_or_else(Body::empty, |value| Body::from(value.to_string())))
            .expect("request")
    }

    fn json_request(uri: &str, payload: serde_json::Value) -> Request<Body> {
        request(Method::POST, uri, Some(payload))
    }

    async fn raw_post_json(app: &Router, uri: &str, payload: serde_json::Value) -> Response {
        app.clone()
            .oneshot(json_request(uri, payload))
            .await
            .expect("response")
    }

    async fn raw_get(app: &Router, uri: &str) -> Response {
        app.clone()
            .oneshot(request(Method::GET, uri, None))
            .await
            .expect("response")
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        app: &Router,
        uri: &str,
        payload: serde_json::Value,
    ) -> T {
        let response = raw_post_json(app, uri, payload).await;
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_str(&response_body_text(response).await).expect("json response")
    }

    async fn get_json<T: serde::de::DeserializeOwned>(app: &Router, uri: &str) -> T {
        let response = raw_get(app, uri).await;
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_str(&response_body_text(response).await).expect("json response")
    }

    async fn response_body_text(response: Response) -> String {
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        String::from_utf8(body.to_vec()).expect("utf8 response")
    }

    fn sample_event_json(device_id: &DeviceId) -> serde_json::Value {
        serde_json::to_value(SyncedEventRecord {
            server_sequence: 0,
            event_id: EventId::new(),
            origin_device_id: *device_id,
            run_id: RunId::new(),
            occurred_at: time::OffsetDateTime::now_utc(),
            kind_type: "run_started".to_string(),
            payload_json: json!({ "sample": true }),
            received_at: time::OffsetDateTime::now_utc(),
        })
        .unwrap()
    }

    #[tokio::test]
    async fn api_registers_device_pushes_and_pulls_events() {
        let app = test_router().await;
        let registered: RegisterDeviceResponse = post_json(
            &app,
            "/api/v1/devices/register",
            serde_json::json!({
                "display_name": "Laptop",
                "public_key_fingerprint": "fp"
            }),
        )
        .await;
        assert_eq!(
            registered.device.public_key_fingerprint.as_deref(),
            Some("fp")
        );
        let devices: Vec<DeviceRecord> = get_json(&app, "/api/v1/devices").await;
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, registered.device.device_id);
        assert_eq!(devices[0].public_key_fingerprint.as_deref(), Some("fp"));

        let push: serde_json::Value = post_json(
            &app,
            "/api/v1/sync/push-events",
            serde_json::json!({
                "device_id": registered.device.device_id,
                "device_token": registered.device_token,
                "events": [sample_event_json(&registered.device.device_id)]
            }),
        )
        .await;
        assert_eq!(push["accepted"], 1);

        let pulled: serde_json::Value = post_json(
            &app,
            "/api/v1/sync/pull-events",
            serde_json::json!({
                "device_id": registered.device.device_id,
                "device_token": registered.device_token,
                "after_server_sequence": 0,
                "limit": 10
            }),
        )
        .await;
        assert_eq!(pulled["events"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn api_rejects_bad_device_token_without_leaking_it() {
        let app = test_router().await;
        let response = raw_post_json(
            &app,
            "/api/v1/sync/pull-events",
            serde_json::json!({
                "device_id": DeviceId::new().to_string(),
                "device_token": "secret-token",
                "after_server_sequence": 0,
                "limit": 10
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response_body_text(response).await;
        assert!(!body.contains("secret-token"));
    }

    #[tokio::test]
    async fn api_exposes_required_server_routes() {
        let app = test_router().await;
        let health: HealthResponse = get_json(&app, "/api/v1/server/health").await;
        assert_eq!(health.status, "ready");

        let registered: RegisterDeviceResponse = post_json(
            &app,
            "/api/v1/devices/register",
            json!({ "display_name": "Phone" }),
        )
        .await;
        let device_id = registered.device.device_id;
        let token = registered.device_token;

        let devices: Vec<DeviceRecord> = get_json(&app, "/api/v1/devices").await;
        assert_eq!(devices.len(), 1);

        let cursors: Vec<SyncCursorRecord> = get_json(&app, "/api/v1/sync/cursors").await;
        assert_eq!(cursors.len(), 1);

        let ack: SyncCursorRecord = post_json(
            &app,
            "/api/v1/sync/ack-events",
            json!({
                "device_id": device_id,
                "device_token": token,
                "last_seen_server_sequence": 0
            }),
        )
        .await;
        assert_eq!(ack.device_id, device_id);

        let objects: serde_json::Value = post_json(
            &app,
            "/api/v1/sync/pull-objects",
            json!({
                "device_id": device_id,
                "device_token": token,
                "object_kind": null,
                "limit": 10
            }),
        )
        .await;
        assert_eq!(objects["objects"].as_array().unwrap().len(), 0);

        let push_objects: serde_json::Value = post_json(
            &app,
            "/api/v1/sync/push-objects",
            json!({
                "device_id": device_id,
                "device_token": token,
                "objects": []
            }),
        )
        .await;
        assert_eq!(push_objects["accepted"], 0);

        let backup: serde_json::Value = post_json(&app, "/api/v1/backups/export", json!({})).await;
        assert!(backup["payload_json"].is_object());

        let backups: Vec<mneme_kernel::BackupRecord> = get_json(&app, "/api/v1/backups").await;
        assert_eq!(backups.len(), 1);

        let imported: mneme_kernel::BackupRecord = post_json(
            &app,
            "/api/v1/backups/import",
            json!({ "payload_json": backup["payload_json"].clone() }),
        )
        .await;
        assert_eq!(imported.event_count, 0);

        let overview: serde_json::Value = get_json(&app, "/api/v1/server/overview").await;
        assert_eq!(overview["devices"], 1);
    }
}
