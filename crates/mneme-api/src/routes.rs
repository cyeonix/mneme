use crate::types::{
    ArtifactDetailResponse, ArtifactsResponse, ContextBundlesResponse, CreateArtifactRequest,
    CreateArtifactResponse, CreateContextBundleRequest, CreateContextBundleResponse,
    CreateKnowledgeSourceRequest, CreateKnowledgeSourceResponse,
    CreateManualKnowledgeSourceRequest, CreateRunRequest, CreateRunResponse,
    DocumentChunksResponse, EventsResponse, HealthResponse, KnowledgeSourcesResponse,
    LifecycleTextRequest, MemoriesResponse, RecordMemoryRequest, RecordMemoryResponse,
    RetrievalTracesResponse, RunLifecycleResponse, RunWorkerModelRequest, RunWorkerModelResponse,
    RunsResponse, SearchRequest, SearchResponseDto, ToolCallRequest, ToolCallResponse,
    ToolLifecycleRequest, ToolLifecycleResponse, ToolResultRequest, WorkerJobsResponse,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use mneme_kernel::{
    kernel_health, ArtifactId, ArtifactServiceError, ChunkTextSplitter, ComposedArtifactService,
    ComposedChunkService, ComposedContextService, ComposedKnowledgeService, ComposedMemoryService,
    ComposedRetrievalService, ComposedRunService, ContextBundleId, ContextServiceError, EventId,
    InMemoryArtifactContentStore, InMemoryArtifactProjectionStore,
    InMemoryContextBundleProjectionStore, InMemoryDocumentChunkProjectionStore,
    InMemoryKnowledgeSourceProjectionStore, InMemoryMemoryProjectionStore, InMemoryOperationLedger,
    InMemoryRetrievalTraceProjectionStore, InMemoryRunProjectionStore, InMemorySearchIndex,
    InMemoryWorkerJobProjectionStore, KnowledgeServiceError, KnowledgeSourceId, MemoryServiceError,
    MockProviderWorker, RetrievalServiceError, RunId, RunServiceError,
    SharedDocumentChunkProjectionStore, SharedKnowledgeSourceProjectionStore,
    SharedMemoryProjectionStore, SharedOperationLedger, SharedRetrievalTraceProjectionStore,
    SharedRunProjectionStore, SharedRunService, SharedSearchIndex, SharedWorkerRuntimeService,
    SqliteArtifactService, SqliteChunkService, SqliteContextService, SqliteKnowledgeService,
    SqliteMemoryService, SqliteRetrievalService, ToolCallId, WorkerId, WorkerRuntimeServiceError,
};

#[derive(Clone)]
pub enum SharedArtifactService {
    Composed(ComposedArtifactService),
    Sqlite(SqliteArtifactService),
}

#[derive(Clone)]
pub enum SharedContextService {
    Composed(ComposedContextService),
    Sqlite(SqliteContextService),
}

#[derive(Clone)]
pub enum SharedKnowledgeService {
    Composed(ComposedKnowledgeService),
    Sqlite(SqliteKnowledgeService),
}

#[derive(Clone)]
pub enum SharedChunkService {
    Composed(ComposedChunkService),
    Sqlite(SqliteChunkService),
}

#[derive(Clone)]
pub enum SharedMemoryService {
    Composed(ComposedMemoryService),
    Sqlite(SqliteMemoryService),
}

#[derive(Clone)]
pub enum SharedRetrievalService {
    Composed(ComposedRetrievalService),
    Sqlite(SqliteRetrievalService),
}

#[derive(Clone)]
pub struct ApiState {
    pub run_service: SharedRunService,
    pub artifact_service: SharedArtifactService,
    pub context_service: SharedContextService,
    pub knowledge_service: SharedKnowledgeService,
    pub chunk_service: SharedChunkService,
    pub memory_service: SharedMemoryService,
    pub retrieval_service: SharedRetrievalService,
    pub worker_runtime_service: SharedWorkerRuntimeService,
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
    let artifact_projection = InMemoryArtifactProjectionStore::new().shared();
    let context_projection = InMemoryContextBundleProjectionStore::new().shared();
    let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();
    let chunk_projection = InMemoryDocumentChunkProjectionStore::new().shared();
    let memory_projection = InMemoryMemoryProjectionStore::new().shared();
    let trace_projection = InMemoryRetrievalTraceProjectionStore::new().shared();
    let search_index = InMemorySearchIndex::new().shared();
    let artifact_service = ComposedArtifactService::new(
        ledger.clone(),
        run_store.clone(),
        artifact_projection.clone(),
        InMemoryArtifactContentStore::new().shared(),
    );
    let context_service = ComposedContextService::new(
        ledger.clone(),
        run_store.clone(),
        artifact_projection.clone(),
        context_projection.clone(),
    );
    let (knowledge_service, chunk_service, memory_service, retrieval_service) =
        composed_m6_services(
            ledger.clone(),
            run_store.clone(),
            artifact_projection,
            context_projection,
            source_projection,
            chunk_projection,
            memory_projection,
            trace_projection,
            search_index,
        );
    let run_service = ComposedRunService::new(ledger.clone(), run_store.clone()).shared();
    let worker_runtime_service = mneme_kernel::ComposedWorkerRuntimeService::new(
        WorkerId::new(),
        "mock worker completion".to_string(),
        ledger,
        run_service.clone(),
        InMemoryWorkerJobProjectionStore::new().shared(),
        MockProviderWorker::new("mock worker completion").shared(),
    )
    .shared();
    build_router_with_all_services(
        run_service,
        SharedArtifactService::Composed(artifact_service),
        SharedContextService::Composed(context_service),
        knowledge_service,
        chunk_service,
        memory_service,
        retrieval_service,
        worker_runtime_service,
    )
}

pub fn build_router_with_service(run_service: SharedRunService) -> Router {
    let ledger = InMemoryOperationLedger::new().shared();
    let run_store = InMemoryRunProjectionStore::new().shared();
    let artifact_projection = InMemoryArtifactProjectionStore::new().shared();
    let context_projection = InMemoryContextBundleProjectionStore::new().shared();
    let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();
    let chunk_projection = InMemoryDocumentChunkProjectionStore::new().shared();
    let memory_projection = InMemoryMemoryProjectionStore::new().shared();
    let trace_projection = InMemoryRetrievalTraceProjectionStore::new().shared();
    let search_index = InMemorySearchIndex::new().shared();
    let artifact_service = ComposedArtifactService::new(
        ledger.clone(),
        run_store.clone(),
        artifact_projection.clone(),
        InMemoryArtifactContentStore::new().shared(),
    );
    let context_service = ComposedContextService::new(
        ledger.clone(),
        run_store.clone(),
        artifact_projection.clone(),
        context_projection.clone(),
    );
    let (knowledge_service, chunk_service, memory_service, retrieval_service) =
        composed_m6_services(
            ledger.clone(),
            run_store,
            artifact_projection,
            context_projection,
            source_projection,
            chunk_projection,
            memory_projection,
            trace_projection,
            search_index,
        );
    let worker_runtime_service = mneme_kernel::ComposedWorkerRuntimeService::new(
        WorkerId::new(),
        "mock worker completion".to_string(),
        ledger.clone(),
        run_service.clone(),
        InMemoryWorkerJobProjectionStore::new().shared(),
        MockProviderWorker::new("mock worker completion").shared(),
    )
    .shared();
    build_router_with_all_services(
        run_service,
        SharedArtifactService::Composed(artifact_service),
        SharedContextService::Composed(context_service),
        knowledge_service,
        chunk_service,
        memory_service,
        retrieval_service,
        worker_runtime_service,
    )
}

#[allow(clippy::too_many_arguments)]
fn composed_m6_services(
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
    artifact_projection: mneme_kernel::SharedArtifactProjectionStore,
    context_projection: mneme_kernel::SharedContextBundleProjectionStore,
    source_projection: SharedKnowledgeSourceProjectionStore,
    chunk_projection: SharedDocumentChunkProjectionStore,
    memory_projection: SharedMemoryProjectionStore,
    trace_projection: SharedRetrievalTraceProjectionStore,
    search_index: SharedSearchIndex,
) -> (
    SharedKnowledgeService,
    SharedChunkService,
    SharedMemoryService,
    SharedRetrievalService,
) {
    let knowledge_service = ComposedKnowledgeService::new(
        ledger.clone(),
        run_store.clone(),
        artifact_projection,
        context_projection,
        source_projection.clone(),
    );
    let chunk_service = ComposedChunkService::new(
        ledger.clone(),
        source_projection.clone(),
        chunk_projection.clone(),
        ChunkTextSplitter::new(4096, 256),
    );
    let memory_service = ComposedMemoryService::new(
        ledger.clone(),
        run_store.clone(),
        source_projection.clone(),
        memory_projection.clone(),
    );
    let retrieval_service = ComposedRetrievalService::new(
        ledger,
        run_store,
        source_projection,
        chunk_projection,
        memory_projection,
        trace_projection,
        search_index,
    );

    (
        SharedKnowledgeService::Composed(knowledge_service),
        SharedChunkService::Composed(chunk_service),
        SharedMemoryService::Composed(memory_service),
        SharedRetrievalService::Composed(retrieval_service),
    )
}

pub fn build_router_with_all_services(
    run_service: SharedRunService,
    artifact_service: SharedArtifactService,
    context_service: SharedContextService,
    knowledge_service: SharedKnowledgeService,
    chunk_service: SharedChunkService,
    memory_service: SharedMemoryService,
    retrieval_service: SharedRetrievalService,
    worker_runtime_service: SharedWorkerRuntimeService,
) -> Router {
    Router::new()
        .route("/api/v1/health", get(get_health))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .route("/api/v1/artifacts", get(list_artifacts))
        .route(
            "/api/v1/runs/:run_id/artifacts",
            axum::routing::post(create_artifact),
        )
        .route("/api/v1/artifacts/:artifact_id", get(get_artifact))
        .route("/api/v1/context-bundles", get(list_context_bundles))
        .route(
            "/api/v1/runs/:run_id/context-bundles",
            axum::routing::post(create_context_bundle),
        )
        .route(
            "/api/v1/context-bundles/:context_bundle_id",
            get(get_context_bundle),
        )
        .route("/api/v1/knowledge-sources", get(list_knowledge_sources))
        .route(
            "/api/v1/artifacts/:artifact_id/knowledge-source",
            axum::routing::post(create_artifact_knowledge_source),
        )
        .route(
            "/api/v1/context-bundles/:context_bundle_id/knowledge-source",
            axum::routing::post(create_context_bundle_knowledge_source),
        )
        .route(
            "/api/v1/knowledge-sources/manual",
            axum::routing::post(create_manual_knowledge_source),
        )
        .route("/api/v1/document-chunks", get(list_document_chunks))
        .route(
            "/api/v1/knowledge-sources/:knowledge_source_id/chunks",
            axum::routing::post(chunk_knowledge_source),
        )
        .route("/api/v1/memories", get(list_memories).post(record_memory))
        .route("/api/v1/search", axum::routing::post(search))
        .route("/api/v1/retrieval-traces", get(list_retrieval_traces))
        .route("/api/v1/worker-jobs", get(list_worker_jobs))
        .route(
            "/api/v1/runs/:run_id/worker-model-completion",
            axum::routing::post(run_worker_model_completion),
        )
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
        .with_state(ApiState {
            run_service,
            artifact_service,
            context_service,
            knowledge_service,
            chunk_service,
            memory_service,
            retrieval_service,
            worker_runtime_service,
        })
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

fn parse_artifact_id(value: &str) -> Result<ArtifactId, StatusCode> {
    value.parse().map_err(|_| StatusCode::BAD_REQUEST)
}

fn parse_context_bundle_id(value: &str) -> Result<ContextBundleId, StatusCode> {
    value.parse().map_err(|_| StatusCode::BAD_REQUEST)
}

fn parse_event_id(value: &str) -> Result<EventId, StatusCode> {
    value.parse().map_err(|_| StatusCode::BAD_REQUEST)
}

fn parse_knowledge_source_id(value: &str) -> Result<KnowledgeSourceId, StatusCode> {
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

async fn list_artifacts(
    State(state): State<ApiState>,
) -> Result<Json<ArtifactsResponse>, StatusCode> {
    let artifacts = match state.artifact_service {
        SharedArtifactService::Composed(service) => service.list_artifacts().await,
        SharedArtifactService::Sqlite(service) => service.list_artifacts().await,
    }
    .map_err(artifact_error_to_status)?;

    Ok(Json(ArtifactsResponse { artifacts }))
}

async fn create_artifact(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<CreateArtifactRequest>,
) -> Result<Json<CreateArtifactResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = match state.artifact_service {
        SharedArtifactService::Composed(service) => {
            service
                .create_text_artifact(
                    run_id,
                    request.title,
                    request.kind,
                    request.mime_type,
                    request.content,
                )
                .await
        }
        SharedArtifactService::Sqlite(service) => {
            service
                .create_text_artifact(
                    run_id,
                    request.title,
                    request.kind,
                    request.mime_type,
                    request.content,
                )
                .await
        }
    }
    .map_err(artifact_error_to_status)?;

    Ok(Json(CreateArtifactResponse {
        artifact_id: outcome.artifact_id.to_string(),
    }))
}

async fn get_artifact(
    State(state): State<ApiState>,
    Path(artifact_id): Path<String>,
) -> Result<Json<ArtifactDetailResponse>, StatusCode> {
    let artifact_id = parse_artifact_id(&artifact_id)?;
    let detail = match state.artifact_service {
        SharedArtifactService::Composed(service) => service.get_artifact(artifact_id).await,
        SharedArtifactService::Sqlite(service) => service.get_artifact(artifact_id).await,
    }
    .map_err(artifact_error_to_status)?;

    Ok(Json(detail.into()))
}

async fn list_context_bundles(
    State(state): State<ApiState>,
) -> Result<Json<ContextBundlesResponse>, StatusCode> {
    let context_bundles = match state.context_service {
        SharedContextService::Composed(service) => service.list_context_bundles().await,
        SharedContextService::Sqlite(service) => service.list_context_bundles().await,
    }
    .map_err(context_error_to_status)?;

    Ok(Json(ContextBundlesResponse { context_bundles }))
}

async fn create_context_bundle(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<CreateContextBundleRequest>,
) -> Result<Json<CreateContextBundleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let CreateContextBundleRequest {
        title,
        summary,
        artifact_ids,
        event_ids,
    } = request;
    let artifact_ids = artifact_ids
        .iter()
        .map(|artifact_id| parse_artifact_id(artifact_id))
        .collect::<Result<Vec<_>, _>>()?;
    let event_ids = event_ids
        .iter()
        .map(|event_id| parse_event_id(event_id))
        .collect::<Result<Vec<_>, _>>()?;
    let outcome = match state.context_service {
        SharedContextService::Composed(service) => {
            service
                .create_context_bundle(run_id, title, summary, artifact_ids, event_ids)
                .await
        }
        SharedContextService::Sqlite(service) => {
            service
                .create_context_bundle(run_id, title, summary, artifact_ids, event_ids)
                .await
        }
    }
    .map_err(context_error_to_status)?;

    Ok(Json(CreateContextBundleResponse {
        context_bundle_id: outcome.context_bundle_id.to_string(),
    }))
}

async fn get_context_bundle(
    State(state): State<ApiState>,
    Path(context_bundle_id): Path<String>,
) -> Result<Json<mneme_kernel::ContextBundleRecord>, StatusCode> {
    let context_bundle_id = parse_context_bundle_id(&context_bundle_id)?;
    let context_bundle = match state.context_service {
        SharedContextService::Composed(service) => {
            service.get_context_bundle(context_bundle_id).await
        }
        SharedContextService::Sqlite(service) => {
            service.get_context_bundle(context_bundle_id).await
        }
    }
    .map_err(context_error_to_status)?;

    Ok(Json(context_bundle))
}

async fn list_knowledge_sources(
    State(state): State<ApiState>,
) -> Result<Json<KnowledgeSourcesResponse>, StatusCode> {
    let knowledge_sources = match state.knowledge_service {
        SharedKnowledgeService::Composed(service) => service.list_sources().await,
        SharedKnowledgeService::Sqlite(service) => service.list_sources().await,
    }
    .map_err(knowledge_error_to_status)?;

    Ok(Json(KnowledgeSourcesResponse { knowledge_sources }))
}

async fn create_artifact_knowledge_source(
    State(state): State<ApiState>,
    Path(artifact_id): Path<String>,
    Json(request): Json<CreateKnowledgeSourceRequest>,
) -> Result<Json<CreateKnowledgeSourceResponse>, StatusCode> {
    let artifact_id = parse_artifact_id(&artifact_id)?;
    let outcome = match state.knowledge_service {
        SharedKnowledgeService::Composed(service) => {
            service
                .create_source_from_artifact(artifact_id, request.title, request.summary)
                .await
        }
        SharedKnowledgeService::Sqlite(service) => {
            service
                .create_source_from_artifact(artifact_id, request.title, request.summary)
                .await
        }
    }
    .map_err(knowledge_error_to_status)?;

    Ok(Json(CreateKnowledgeSourceResponse {
        knowledge_source_id: outcome.knowledge_source_id.to_string(),
    }))
}

async fn create_context_bundle_knowledge_source(
    State(state): State<ApiState>,
    Path(context_bundle_id): Path<String>,
    Json(request): Json<CreateKnowledgeSourceRequest>,
) -> Result<Json<CreateKnowledgeSourceResponse>, StatusCode> {
    let context_bundle_id = parse_context_bundle_id(&context_bundle_id)?;
    let outcome = match state.knowledge_service {
        SharedKnowledgeService::Composed(service) => {
            service
                .create_source_from_context_bundle(
                    context_bundle_id,
                    request.title,
                    request.summary,
                )
                .await
        }
        SharedKnowledgeService::Sqlite(service) => {
            service
                .create_source_from_context_bundle(
                    context_bundle_id,
                    request.title,
                    request.summary,
                )
                .await
        }
    }
    .map_err(knowledge_error_to_status)?;

    Ok(Json(CreateKnowledgeSourceResponse {
        knowledge_source_id: outcome.knowledge_source_id.to_string(),
    }))
}

async fn create_manual_knowledge_source(
    State(state): State<ApiState>,
    Json(request): Json<CreateManualKnowledgeSourceRequest>,
) -> Result<Json<CreateKnowledgeSourceResponse>, StatusCode> {
    let run_id = parse_run_id(&request.run_id)?;
    let outcome = match state.knowledge_service {
        SharedKnowledgeService::Composed(service) => {
            service
                .create_manual_source(run_id, request.title, request.summary)
                .await
        }
        SharedKnowledgeService::Sqlite(service) => {
            service
                .create_manual_source(run_id, request.title, request.summary)
                .await
        }
    }
    .map_err(knowledge_error_to_status)?;

    Ok(Json(CreateKnowledgeSourceResponse {
        knowledge_source_id: outcome.knowledge_source_id.to_string(),
    }))
}

#[derive(Debug, serde::Deserialize)]
struct KnowledgeSourceFilter {
    knowledge_source_id: Option<String>,
}

async fn list_document_chunks(
    State(state): State<ApiState>,
    Query(filter): Query<KnowledgeSourceFilter>,
) -> Result<Json<DocumentChunksResponse>, StatusCode> {
    let knowledge_source_id = filter
        .knowledge_source_id
        .as_deref()
        .map(parse_knowledge_source_id)
        .transpose()?;
    let document_chunks = match state.chunk_service {
        SharedChunkService::Composed(service) => service.list_chunks(knowledge_source_id).await,
        SharedChunkService::Sqlite(service) => service.list_chunks(knowledge_source_id).await,
    }
    .map_err(knowledge_error_to_status)?;

    Ok(Json(DocumentChunksResponse { document_chunks }))
}

async fn chunk_knowledge_source(
    State(state): State<ApiState>,
    Path(knowledge_source_id): Path<String>,
) -> Result<Json<DocumentChunksResponse>, StatusCode> {
    let knowledge_source_id = parse_knowledge_source_id(&knowledge_source_id)?;
    let document_chunks = match state.chunk_service {
        SharedChunkService::Composed(service) => service.chunk_source(knowledge_source_id).await,
        SharedChunkService::Sqlite(service) => service.chunk_source(knowledge_source_id).await,
    }
    .map_err(knowledge_error_to_status)?;

    Ok(Json(DocumentChunksResponse { document_chunks }))
}

async fn list_memories(
    State(state): State<ApiState>,
    Query(filter): Query<KnowledgeSourceFilter>,
) -> Result<Json<MemoriesResponse>, StatusCode> {
    let knowledge_source_id = filter
        .knowledge_source_id
        .as_deref()
        .map(parse_knowledge_source_id)
        .transpose()?;
    let memories = match state.memory_service {
        SharedMemoryService::Composed(service) => service.list_memories(knowledge_source_id).await,
        SharedMemoryService::Sqlite(service) => service.list_memories(knowledge_source_id).await,
    }
    .map_err(memory_error_to_status)?;

    Ok(Json(MemoriesResponse { memories }))
}

async fn record_memory(
    State(state): State<ApiState>,
    Json(request): Json<RecordMemoryRequest>,
) -> Result<Json<RecordMemoryResponse>, StatusCode> {
    let run_id = parse_run_id(&request.run_id)?;
    let knowledge_source_id = request
        .knowledge_source_id
        .as_deref()
        .map(parse_knowledge_source_id)
        .transpose()?;
    let outcome = match state.memory_service {
        SharedMemoryService::Composed(service) => {
            service
                .record_memory(
                    run_id,
                    knowledge_source_id,
                    request.kind,
                    request.title,
                    request.body,
                    request.confidence,
                )
                .await
        }
        SharedMemoryService::Sqlite(service) => {
            service
                .record_memory(
                    run_id,
                    knowledge_source_id,
                    request.kind,
                    request.title,
                    request.body,
                    request.confidence,
                )
                .await
        }
    }
    .map_err(memory_error_to_status)?;

    Ok(Json(RecordMemoryResponse {
        memory_id: outcome.memory_id.to_string(),
    }))
}

async fn search(
    State(state): State<ApiState>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponseDto>, StatusCode> {
    let run_id = parse_run_id(&request.run_id)?;
    let limit = request.limit.unwrap_or(10);
    let response = match state.retrieval_service {
        SharedRetrievalService::Composed(service) => {
            service.search(run_id, request.query, limit).await
        }
        SharedRetrievalService::Sqlite(service) => {
            service.search(run_id, request.query, limit).await
        }
    }
    .map_err(retrieval_error_to_status)?;

    Ok(Json(SearchResponseDto {
        query: response.query,
        hits: response.hits,
        retrieval_trace_id: response.retrieval_trace_id.to_string(),
    }))
}

async fn list_retrieval_traces(
    State(state): State<ApiState>,
) -> Result<Json<RetrievalTracesResponse>, StatusCode> {
    let retrieval_traces = match state.retrieval_service {
        SharedRetrievalService::Composed(service) => service.list_retrieval_traces().await,
        SharedRetrievalService::Sqlite(service) => service.list_retrieval_traces().await,
    }
    .map_err(retrieval_error_to_status)?;

    Ok(Json(RetrievalTracesResponse { retrieval_traces }))
}

async fn list_worker_jobs(
    State(state): State<ApiState>,
) -> Result<Json<WorkerJobsResponse>, StatusCode> {
    let worker_jobs = state
        .worker_runtime_service
        .list_worker_jobs()
        .await
        .map_err(worker_runtime_error_to_status)?;
    Ok(Json(WorkerJobsResponse { worker_jobs }))
}

async fn run_worker_model_completion(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<RunWorkerModelRequest>,
) -> Result<Json<RunWorkerModelResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = state
        .worker_runtime_service
        .run_model_completion(run_id, request.prompt)
        .await
        .map_err(worker_runtime_error_to_status)?;
    Ok(Json(RunWorkerModelResponse {
        run_id: outcome.run_id.to_string(),
        worker_job_id: outcome.worker_job_id.to_string(),
    }))
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

fn artifact_error_to_status(error: ArtifactServiceError) -> StatusCode {
    match error {
        ArtifactServiceError::InvalidInput => StatusCode::BAD_REQUEST,
        ArtifactServiceError::UnknownRun | ArtifactServiceError::UnknownArtifact => {
            StatusCode::NOT_FOUND
        }
        ArtifactServiceError::ContentStore(_)
        | ArtifactServiceError::Ledger(_)
        | ArtifactServiceError::Projection(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn context_error_to_status(error: ContextServiceError) -> StatusCode {
    match error {
        ContextServiceError::InvalidInput => StatusCode::BAD_REQUEST,
        ContextServiceError::UnknownRun
        | ContextServiceError::UnknownContextBundle
        | ContextServiceError::UnknownArtifact
        | ContextServiceError::UnknownEvent => StatusCode::NOT_FOUND,
        ContextServiceError::Ledger(_) | ContextServiceError::Projection(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn knowledge_error_to_status(error: KnowledgeServiceError) -> StatusCode {
    match error {
        KnowledgeServiceError::InvalidInput => StatusCode::BAD_REQUEST,
        KnowledgeServiceError::UnknownRun
        | KnowledgeServiceError::UnknownArtifact
        | KnowledgeServiceError::UnknownContextBundle
        | KnowledgeServiceError::UnknownKnowledgeSource => StatusCode::NOT_FOUND,
        KnowledgeServiceError::Ledger(_) | KnowledgeServiceError::Projection(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn memory_error_to_status(error: MemoryServiceError) -> StatusCode {
    match error {
        MemoryServiceError::InvalidInput => StatusCode::BAD_REQUEST,
        MemoryServiceError::UnknownRun
        | MemoryServiceError::UnknownKnowledgeSource
        | MemoryServiceError::UnknownMemory => StatusCode::NOT_FOUND,
        MemoryServiceError::Ledger(_) | MemoryServiceError::Projection(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn retrieval_error_to_status(error: RetrievalServiceError) -> StatusCode {
    match error {
        RetrievalServiceError::InvalidInput => StatusCode::BAD_REQUEST,
        RetrievalServiceError::UnknownRun => StatusCode::NOT_FOUND,
        RetrievalServiceError::Index(_)
        | RetrievalServiceError::Ledger(_)
        | RetrievalServiceError::Projection(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn worker_runtime_error_to_status(error: WorkerRuntimeServiceError) -> StatusCode {
    match error {
        WorkerRuntimeServiceError::InvalidInput | WorkerRuntimeServiceError::RunAlreadyTerminal => {
            StatusCode::BAD_REQUEST
        }
        WorkerRuntimeServiceError::UnknownRun => StatusCode::NOT_FOUND,
        WorkerRuntimeServiceError::ProviderMissingConfiguration => StatusCode::SERVICE_UNAVAILABLE,
        WorkerRuntimeServiceError::ProviderTimeout => StatusCode::GATEWAY_TIMEOUT,
        WorkerRuntimeServiceError::ProviderBadGateway(_) => StatusCode::BAD_GATEWAY,
        WorkerRuntimeServiceError::Ledger(_)
        | WorkerRuntimeServiceError::Projection(_)
        | WorkerRuntimeServiceError::RunService(_) => StatusCode::INTERNAL_SERVER_ERROR,
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
    use mneme_kernel::{
        ComposedRunService, OperationLedger, ProviderWorkerError, SqliteRunService, SqliteStore,
    };
    use serde_json::json;
    use tower::ServiceExt;

    fn build_router_with_in_memory_services() -> Router {
        build_router_with_state(
            InMemoryOperationLedger::new().shared(),
            InMemoryRunProjectionStore::new().shared(),
        )
    }

    fn build_router_with_in_memory_worker_provider(
        provider: mneme_kernel::SharedProviderWorker,
    ) -> Router {
        let ledger = InMemoryOperationLedger::new().shared();
        let run_store = InMemoryRunProjectionStore::new().shared();
        let run_service = ComposedRunService::new(ledger.clone(), run_store.clone()).shared();
        let worker_runtime_service = mneme_kernel::ComposedWorkerRuntimeService::new(
            WorkerId::new(),
            "mock-worker-model".to_string(),
            ledger.clone(),
            run_service.clone(),
            InMemoryWorkerJobProjectionStore::new().shared(),
            provider,
        )
        .shared();
        let artifact_projection = InMemoryArtifactProjectionStore::new().shared();
        let context_projection = InMemoryContextBundleProjectionStore::new().shared();
        let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();
        let chunk_projection = InMemoryDocumentChunkProjectionStore::new().shared();
        let memory_projection = InMemoryMemoryProjectionStore::new().shared();
        let trace_projection = InMemoryRetrievalTraceProjectionStore::new().shared();
        let search_index = InMemorySearchIndex::new().shared();
        let artifact_service = ComposedArtifactService::new(
            ledger.clone(),
            run_store.clone(),
            artifact_projection.clone(),
            InMemoryArtifactContentStore::new().shared(),
        );
        let context_service = ComposedContextService::new(
            ledger.clone(),
            run_store.clone(),
            artifact_projection.clone(),
            context_projection.clone(),
        );
        let (knowledge_service, chunk_service, memory_service, retrieval_service) =
            composed_m6_services(
                ledger,
                run_store,
                artifact_projection,
                context_projection,
                source_projection,
                chunk_projection,
                memory_projection,
                trace_projection,
                search_index,
            );
        build_router_with_all_services(
            run_service,
            SharedArtifactService::Composed(artifact_service),
            SharedContextService::Composed(context_service),
            knowledge_service,
            chunk_service,
            memory_service,
            retrieval_service,
            worker_runtime_service,
        )
    }

    fn empty_request(method: &str, uri: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("request")
    }

    fn json_request(method: &str, uri: &str, payload: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .expect("request")
    }

    async fn response_json<T: serde::de::DeserializeOwned>(
        response: axum::response::Response,
    ) -> T {
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        serde_json::from_slice(&body).expect("json response")
    }

    async fn create_run_for_test(app: &Router, instruction: &str) -> String {
        let response = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/v1/runs",
                json!({ "instruction": instruction }),
            ))
            .await
            .expect("create run response");
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response_json(response).await;
        body["run_id"].as_str().expect("run_id").to_string()
    }

    #[tokio::test]
    async fn api_runs_worker_model_completion_with_mock_provider() {
        let app = build_router_with_in_memory_worker_provider(
            MockProviderWorker::new("api reply").shared(),
        );
        let run_id = create_run_for_test(&app, "worker run").await;

        let response = app
            .clone()
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/worker-model-completion"),
                serde_json::json!({ "prompt": "hello" }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: RunWorkerModelResponse = response_json(response).await;
        assert_eq!(body.run_id, run_id);

        let jobs = app
            .oneshot(empty_request("GET", "/api/v1/worker-jobs"))
            .await
            .unwrap();
        assert_eq!(jobs.status(), StatusCode::OK);
        let body: WorkerJobsResponse = response_json(jobs).await;
        assert_eq!(body.worker_jobs.len(), 1);
    }

    #[tokio::test]
    async fn api_rejects_blank_worker_prompt() {
        let app = build_router_with_in_memory_worker_provider(
            MockProviderWorker::new("api reply").shared(),
        );
        let run_id = create_run_for_test(&app, "worker run").await;

        let response = app
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/worker-model-completion"),
                serde_json::json!({ "prompt": "   " }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_maps_missing_provider_config_to_service_unavailable() {
        let app = build_router_with_in_memory_worker_provider(
            MockProviderWorker::failing(ProviderWorkerError::MissingConfiguration).shared(),
        );
        let run_id = create_run_for_test(&app, "worker run").await;

        let response = app
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/worker-model-completion"),
                serde_json::json!({ "prompt": "hello" }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

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

    async fn create_artifact_for_test(
        app: &Router,
        run_id: &str,
        title: &str,
        content: &str,
    ) -> String {
        let response = app
            .clone()
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/artifacts"),
                json!({
                    "title": title,
                    "kind": "markdown",
                    "mime_type": "text/markdown",
                    "content": content,
                }),
            ))
            .await
            .expect("create artifact response");
        assert_eq!(response.status(), StatusCode::OK);
        let body: CreateArtifactResponse = response_json(response).await;
        body.artifact_id
    }

    #[tokio::test]
    async fn api_creates_knowledge_chunks_searches_and_lists_retrieval_traces() {
        let app = build_router_with_in_memory_services();
        let run_id = create_run_for_test(&app, "Search run").await;
        let artifact_id = create_artifact_for_test(
            &app,
            &run_id,
            "Searchable artifact",
            "SQLite deterministic retrieval",
        )
        .await;

        let source_response: CreateKnowledgeSourceResponse = response_json(
            app.clone()
                .oneshot(json_request(
                    "POST",
                    &format!("/api/v1/artifacts/{artifact_id}/knowledge-source"),
                    serde_json::json!({"title":"Searchable artifact","summary":"SQLite deterministic retrieval"}),
                ))
                .await
                .unwrap(),
        )
        .await;

        let chunks_response: DocumentChunksResponse = response_json(
            app.clone()
                .oneshot(empty_request(
                    "POST",
                    &format!(
                        "/api/v1/knowledge-sources/{}/chunks",
                        source_response.knowledge_source_id
                    ),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(chunks_response.document_chunks.len(), 1);

        let search_response: SearchResponseDto = response_json(
            app.clone()
                .oneshot(json_request(
                    "POST",
                    "/api/v1/search",
                    serde_json::json!({"run_id":run_id,"query":"deterministic","limit":5}),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(search_response.hits.len(), 1);

        let traces: RetrievalTracesResponse = response_json(
            app.oneshot(empty_request("GET", "/api/v1/retrieval-traces"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(traces.retrieval_traces.len(), 1);
    }

    #[tokio::test]
    async fn api_rejects_blank_search_query() {
        let app = build_router_with_in_memory_services();
        let run_id = create_run_for_test(&app, "Search run").await;
        let response = app
            .oneshot(json_request(
                "POST",
                "/api/v1/search",
                serde_json::json!({"run_id":run_id,"query":"   ","limit":5}),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_creates_lists_and_gets_artifact() {
        let app = build_router_with_in_memory_services();
        let run_id = create_run_for_test(&app, "collect artifact").await;

        let create_response = app
            .clone()
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/artifacts"),
                serde_json::json!({
                    "title": "Run summary",
                    "kind": "markdown",
                    "mime_type": "text/markdown",
                    "content": "# Summary"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let body: serde_json::Value = response_json(create_response).await;
        let artifact_id = body["artifact_id"].as_str().unwrap();

        let list_response = app
            .clone()
            .oneshot(empty_request("GET", "/api/v1/artifacts"))
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list: serde_json::Value = response_json(list_response).await;
        assert_eq!(list["artifacts"].as_array().unwrap().len(), 1);

        let get_response = app
            .oneshot(empty_request(
                "GET",
                &format!("/api/v1/artifacts/{artifact_id}"),
            ))
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let detail: serde_json::Value = response_json(get_response).await;
        assert_eq!(detail["artifact"]["title"], "Run summary");
        assert_eq!(detail["content"], "# Summary");
    }

    #[tokio::test]
    async fn api_rejects_invalid_artifact_input() {
        let app = build_router_with_in_memory_services();
        let run_id = create_run_for_test(&app, "reject artifact").await;

        let response = app
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/artifacts"),
                serde_json::json!({
                    "title": " ",
                    "kind": "markdown",
                    "mime_type": "text/markdown",
                    "content": "# Summary"
                }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_rejects_invalid_artifact_id() {
        let app = build_router_with_in_memory_services();

        let response = app
            .oneshot(empty_request("GET", "/api/v1/artifacts/not-a-uuid"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_returns_not_found_for_unknown_artifact() {
        let app = build_router_with_in_memory_services();
        let artifact_id = mneme_kernel::ArtifactId::new();

        let response = app
            .oneshot(empty_request(
                "GET",
                &format!("/api/v1/artifacts/{artifact_id}"),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_creates_lists_and_gets_context_bundle() {
        let app = build_router_with_in_memory_services();
        let run_id = create_run_for_test(&app, "collect context").await;

        let create_response = app
            .clone()
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/context-bundles"),
                serde_json::json!({
                    "title": "Debug context",
                    "summary": "User report and selected events",
                    "artifact_ids": [],
                    "event_ids": []
                }),
            ))
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        let body: serde_json::Value = response_json(create_response).await;
        let context_bundle_id = body["context_bundle_id"].as_str().unwrap();

        let list_response = app
            .clone()
            .oneshot(empty_request("GET", "/api/v1/context-bundles"))
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list: serde_json::Value = response_json(list_response).await;
        assert_eq!(list["context_bundles"].as_array().unwrap().len(), 1);

        let get_response = app
            .oneshot(empty_request(
                "GET",
                &format!("/api/v1/context-bundles/{context_bundle_id}"),
            ))
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let detail: serde_json::Value = response_json(get_response).await;
        assert_eq!(detail["title"], "Debug context");
        assert_eq!(detail["artifact_ids"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn api_rejects_invalid_context_bundle_reference() {
        let app = build_router_with_in_memory_services();
        let run_id = create_run_for_test(&app, "reject context").await;

        let response = app
            .oneshot(json_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/context-bundles"),
                serde_json::json!({
                    "title": "Debug context",
                    "summary": "User report and selected events",
                    "artifact_ids": ["not-a-uuid"],
                    "event_ids": []
                }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_returns_not_found_for_unknown_context_bundle() {
        let app = build_router_with_in_memory_services();
        let context_bundle_id = mneme_kernel::ContextBundleId::new();

        let response = app
            .oneshot(empty_request(
                "GET",
                &format!("/api/v1/context-bundles/{context_bundle_id}"),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
