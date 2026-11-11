use async_trait::async_trait;
use mneme_kernel::{DeviceId, DeviceRecord, SyncCursorRecord, SyncedEventRecord};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

const REGISTER_DEVICE_PATH: &str = "/api/v1/devices/register";
const PUSH_EVENTS_PATH: &str = "/api/v1/sync/push-events";
const PULL_EVENTS_PATH: &str = "/api/v1/sync/pull-events";
const ACK_EVENTS_PATH: &str = "/api/v1/sync/ack-events";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyncClientError {
    #[error("invalid input")]
    InvalidInput,
    #[error("http error: {0}")]
    Http(String),
    #[error("server returned status {status}")]
    Status { status: u16 },
    #[error("malformed response: {0}")]
    MalformedResponse(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeviceToken(String);

impl DeviceToken {
    pub fn new(value: String) -> Result<Self, SyncClientError> {
        if value.trim().is_empty() {
            return Err(SyncClientError::InvalidInput);
        }
        Ok(Self(value))
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DeviceToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeviceToken(<redacted>)")
    }
}

impl Serialize for DeviceToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DeviceToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        DeviceToken::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RedactedDeviceToken<'a>(&'a DeviceToken);

impl<'a> RedactedDeviceToken<'a> {
    pub fn expose_secret(&self) -> &'a str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for RedactedDeviceToken<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeviceToken(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterDeviceResponse {
    pub device: DeviceRecord,
    pub cursor: SyncCursorRecord,
    pub device_token: DeviceToken,
}

impl RegisterDeviceResponse {
    pub fn redacted_device_token(&self) -> RedactedDeviceToken<'_> {
        RedactedDeviceToken(&self.device_token)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct RegisterDeviceRequest {
    display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_key_fingerprint: Option<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PushEventsRequest {
    pub device_id: DeviceId,
    pub device_token: DeviceToken,
    pub events: Vec<SyncedEventRecord>,
}

impl fmt::Debug for PushEventsRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PushEventsRequest")
            .field("device_id", &self.device_id)
            .field("device_token", &self.device_token)
            .field("events", &self.events)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushEventsResponse {
    pub accepted: usize,
    pub duplicates: usize,
    pub latest_server_sequence: i64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PullEventsRequest {
    pub device_id: DeviceId,
    pub device_token: DeviceToken,
    pub after_server_sequence: i64,
    pub limit: usize,
}

impl fmt::Debug for PullEventsRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PullEventsRequest")
            .field("device_id", &self.device_id)
            .field("device_token", &self.device_token)
            .field("after_server_sequence", &self.after_server_sequence)
            .field("limit", &self.limit)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullEventsResponse {
    pub events: Vec<SyncedEventRecord>,
    pub latest_server_sequence: i64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AckEventsRequest {
    pub device_id: DeviceId,
    pub device_token: DeviceToken,
    pub last_seen_server_sequence: i64,
}

impl fmt::Debug for AckEventsRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AckEventsRequest")
            .field("device_id", &self.device_id)
            .field("device_token", &self.device_token)
            .field("last_seen_server_sequence", &self.last_seen_server_sequence)
            .finish()
    }
}

#[async_trait]
pub trait SyncClient: Send + Sync {
    async fn register_device(
        &self,
        display_name: String,
    ) -> Result<RegisterDeviceResponse, SyncClientError>;
    async fn push_events(
        &self,
        request: PushEventsRequest,
    ) -> Result<PushEventsResponse, SyncClientError>;
    async fn pull_events(
        &self,
        request: PullEventsRequest,
    ) -> Result<PullEventsResponse, SyncClientError>;
    async fn ack_events(
        &self,
        request: AckEventsRequest,
    ) -> Result<SyncCursorRecord, SyncClientError>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct SyncTransportResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

impl fmt::Debug for SyncTransportResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyncTransportResponse")
            .field("status", &self.status)
            .field("body", &redact_json_tokens(&self.body))
            .finish()
    }
}

#[async_trait]
pub trait SyncTransport: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<SyncTransportResponse, SyncClientError>;
}

#[derive(Debug, Clone)]
pub struct HttpSyncClient<T = ReqwestSyncTransport> {
    base_url: String,
    transport: T,
}

impl HttpSyncClient<ReqwestSyncTransport> {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_transport(base_url, ReqwestSyncTransport::default())
    }
}

impl<T> HttpSyncClient<T> {
    pub fn with_transport(base_url: impl Into<String>, transport: T) -> Self {
        Self {
            base_url: normalize_base_url(base_url.into()),
            transport,
        }
    }
}

#[async_trait]
impl<T> SyncClient for HttpSyncClient<T>
where
    T: SyncTransport,
{
    async fn register_device(
        &self,
        display_name: String,
    ) -> Result<RegisterDeviceResponse, SyncClientError> {
        if display_name.trim().is_empty() {
            return Err(SyncClientError::InvalidInput);
        }
        self.post(
            REGISTER_DEVICE_PATH,
            RegisterDeviceRequest {
                display_name,
                public_key_fingerprint: None,
            },
        )
        .await
    }

    async fn push_events(
        &self,
        request: PushEventsRequest,
    ) -> Result<PushEventsResponse, SyncClientError> {
        self.post(PUSH_EVENTS_PATH, request).await
    }

    async fn pull_events(
        &self,
        request: PullEventsRequest,
    ) -> Result<PullEventsResponse, SyncClientError> {
        if request.limit == 0 {
            return Err(SyncClientError::InvalidInput);
        }
        self.post(PULL_EVENTS_PATH, request).await
    }

    async fn ack_events(
        &self,
        request: AckEventsRequest,
    ) -> Result<SyncCursorRecord, SyncClientError> {
        self.post(ACK_EVENTS_PATH, request).await
    }
}

impl<T> HttpSyncClient<T>
where
    T: SyncTransport,
{
    async fn post<Request, Response>(
        &self,
        path: &str,
        request: Request,
    ) -> Result<Response, SyncClientError>
    where
        Request: Serialize + Send + Sync,
        Response: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        let body = serde_json::to_value(request)
            .map_err(|error| SyncClientError::MalformedResponse(error.to_string()))?;
        let response = self.transport.post_json(&url, body).await?;
        if !(200..300).contains(&response.status) {
            return Err(SyncClientError::Status {
                status: response.status,
            });
        }
        serde_json::from_value(response.body)
            .map_err(|error| SyncClientError::MalformedResponse(error.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct ReqwestSyncTransport {
    client: reqwest::Client,
}

impl Default for ReqwestSyncTransport {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl ReqwestSyncTransport {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SyncTransport for ReqwestSyncTransport {
    async fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<SyncTransportResponse, SyncClientError> {
        let response = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|error| SyncClientError::Http(error.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Ok(SyncTransportResponse {
                status,
                body: serde_json::Value::Null,
            });
        }
        let body = response
            .json::<serde_json::Value>()
            .await
            .map_err(|error| SyncClientError::MalformedResponse(error.to_string()))?;
        Ok(SyncTransportResponse { status, body })
    }
}

fn normalize_base_url(base_url: String) -> String {
    base_url.trim_end_matches('/').to_string()
}

fn redact_json_tokens(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| {
                let redacted = if key == "device_token" {
                    serde_json::Value::String("<redacted>".to_string())
                } else {
                    redact_json_tokens(value)
                };
                (key.clone(), redacted)
            })
            .collect(),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_json_tokens).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct MockSyncTransport {
        inner: Arc<Mutex<MockState>>,
    }

    #[derive(Debug)]
    struct MockState {
        status: u16,
        response: serde_json::Value,
        requests: Vec<RecordedRequest>,
    }

    #[derive(Clone)]
    struct RecordedRequest {
        path: String,
        body: serde_json::Value,
    }

    impl fmt::Debug for RecordedRequest {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("RecordedRequest")
                .field("path", &self.path)
                .field("body", &redact_json_tokens(&self.body))
                .finish()
        }
    }

    impl fmt::Debug for MockSyncTransport {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            let inner = self.inner.lock().expect("mock transport mutex poisoned");
            formatter
                .debug_struct("MockSyncTransport")
                .field("status", &inner.status)
                .field("response", &redact_json_tokens(&inner.response))
                .field("requests", &inner.requests)
                .finish()
        }
    }

    impl MockSyncTransport {
        pub fn new(response: serde_json::Value) -> Self {
            Self::with_status(200, response)
        }

        pub fn with_status(status: u16, response: serde_json::Value) -> Self {
            Self {
                inner: Arc::new(Mutex::new(MockState {
                    status,
                    response,
                    requests: Vec::new(),
                })),
            }
        }

        pub fn set_response(&self, response: serde_json::Value) {
            let mut inner = self.inner.lock().expect("mock transport mutex poisoned");
            inner.status = 200;
            inner.response = response;
        }

        pub fn last_path(&self) -> Option<String> {
            self.inner
                .lock()
                .expect("mock transport mutex poisoned")
                .requests
                .last()
                .map(|request| request.path.clone())
        }

        pub fn last_body(&self) -> Option<serde_json::Value> {
            self.inner
                .lock()
                .expect("mock transport mutex poisoned")
                .requests
                .last()
                .map(|request| request.body.clone())
        }

        pub fn request_count(&self) -> usize {
            self.inner
                .lock()
                .expect("mock transport mutex poisoned")
                .requests
                .len()
        }
    }

    #[async_trait]
    impl SyncTransport for MockSyncTransport {
        async fn post_json(
            &self,
            url: &str,
            body: serde_json::Value,
        ) -> Result<SyncTransportResponse, SyncClientError> {
            let path = reqwest::Url::parse(url)
                .map_err(|error| SyncClientError::Http(error.to_string()))?
                .path()
                .to_string();
            let mut inner = self.inner.lock().expect("mock transport mutex poisoned");
            inner.requests.push(RecordedRequest { path, body });
            Ok(SyncTransportResponse {
                status: inner.status,
                body: inner.response.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mneme_kernel::{DeviceId, SyncCursorRecord};
    use serde_json::json;
    use test_support::MockSyncTransport;
    use time::OffsetDateTime;

    #[tokio::test]
    async fn http_sync_client_posts_register_device_request() {
        let device_id = DeviceId::new();
        let transport = MockSyncTransport::new(json!({
            "device": {
                "device_id": device_id,
                "display_name": "Laptop",
                "registered_at": OffsetDateTime::now_utc(),
                "last_seen_at": OffsetDateTime::now_utc()
            },
            "cursor": {
                "device_id": device_id,
                "last_server_sequence": 0,
                "updated_at": OffsetDateTime::now_utc()
            },
            "device_token": "mneme-dev-token-test-secret"
        }));
        let client = HttpSyncClient::with_transport("http://127.0.0.1:3866", transport.clone());

        let response = client.register_device("Laptop".to_string()).await.unwrap();

        assert!(!response.device_token.expose_secret().is_empty());
        assert_eq!(transport.last_path().unwrap(), "/api/v1/devices/register");
        assert_eq!(transport.last_body().unwrap()["display_name"], "Laptop");
        assert!(!format!("{:?}", response).contains("test-secret"));
    }

    #[tokio::test]
    async fn http_sync_client_posts_authenticated_event_requests_with_token() {
        let device_id = DeviceId::new();
        let token = DeviceToken::new("mneme-dev-token-test-secret".to_string()).unwrap();
        let transport = MockSyncTransport::new(json!({
            "events": [],
            "latest_server_sequence": 7
        }));
        let client = HttpSyncClient::with_transport("http://127.0.0.1:3866", transport.clone());

        let response = client
            .pull_events(PullEventsRequest {
                device_id,
                device_token: token,
                after_server_sequence: 3,
                limit: 10,
            })
            .await
            .unwrap();

        assert_eq!(response.latest_server_sequence, 7);
        assert_eq!(transport.last_path().unwrap(), "/api/v1/sync/pull-events");
        assert_eq!(
            transport.last_body().unwrap()["device_id"],
            device_id.to_string()
        );
        assert_eq!(
            transport.last_body().unwrap()["device_token"],
            "mneme-dev-token-test-secret"
        );
        assert!(!format!("{:?}", transport).contains("test-secret"));
    }

    #[tokio::test]
    async fn http_sync_client_maps_non_success_status_without_body_leak() {
        let transport = MockSyncTransport::with_status(
            401,
            json!({
                "error": "unauthorized",
                "device_token": "mneme-dev-token-test-secret"
            }),
        );
        let client = HttpSyncClient::with_transport("http://127.0.0.1:3866", transport);

        let error = client
            .register_device("Laptop".to_string())
            .await
            .unwrap_err();

        assert_eq!(error, SyncClientError::Status { status: 401 });
        assert!(!format!("{error:?}").contains("test-secret"));
    }

    #[tokio::test]
    async fn http_sync_client_posts_push_and_ack_paths() {
        let device_id = DeviceId::new();
        let token = DeviceToken::new("mneme-dev-token-test-secret".to_string()).unwrap();
        let transport = MockSyncTransport::new(json!({
            "accepted": 0,
            "duplicates": 0,
            "latest_server_sequence": 0
        }));
        let client = HttpSyncClient::with_transport("http://127.0.0.1:3866", transport.clone());

        let push = client
            .push_events(PushEventsRequest {
                device_id,
                device_token: token.clone(),
                events: Vec::new(),
            })
            .await
            .unwrap();

        assert_eq!(push.accepted, 0);
        assert_eq!(transport.last_path().unwrap(), "/api/v1/sync/push-events");

        transport.set_response(json!({
            "device_id": device_id,
            "last_server_sequence": 42,
            "updated_at": OffsetDateTime::now_utc()
        }));
        let ack = client
            .ack_events(AckEventsRequest {
                device_id,
                device_token: token,
                last_seen_server_sequence: 42,
            })
            .await
            .unwrap();

        assert_eq!(ack.last_server_sequence, 42);
        assert_eq!(transport.last_path().unwrap(), "/api/v1/sync/ack-events");
    }

    #[tokio::test]
    async fn http_sync_client_rejects_empty_inputs_before_transport() {
        let transport = MockSyncTransport::new(json!({}));
        let client = HttpSyncClient::with_transport("http://127.0.0.1:3866", transport.clone());

        let error = client.register_device("  ".to_string()).await.unwrap_err();

        assert_eq!(error, SyncClientError::InvalidInput);
        assert_eq!(transport.request_count(), 0);
        assert_eq!(
            DeviceToken::new("".to_string()).unwrap_err(),
            SyncClientError::InvalidInput
        );
    }

    #[tokio::test]
    async fn device_token_rejects_empty_response_tokens() {
        let device_id = DeviceId::new();
        let transport = MockSyncTransport::new(json!({
            "device": {
                "device_id": device_id,
                "display_name": "Laptop",
                "registered_at": OffsetDateTime::now_utc(),
                "last_seen_at": OffsetDateTime::now_utc()
            },
            "cursor": {
                "device_id": device_id,
                "last_server_sequence": 0,
                "updated_at": OffsetDateTime::now_utc()
            },
            "device_token": ""
        }));
        let client = HttpSyncClient::with_transport("http://127.0.0.1:3866", transport);

        let error = client
            .register_device("Laptop".to_string())
            .await
            .unwrap_err();

        assert!(matches!(error, SyncClientError::MalformedResponse(_)));
    }

    #[allow(dead_code)]
    fn _assert_sync_cursor_returned(cursor: SyncCursorRecord) -> SyncCursorRecord {
        cursor
    }
}
