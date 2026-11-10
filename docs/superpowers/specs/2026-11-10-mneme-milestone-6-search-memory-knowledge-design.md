# Mneme Milestone 6：Search / Memory / Knowledge 设计

## 1. 背景

Milestone 1–5 已经完成 Mneme Phase 1 的本地持久化数据面、Kernel 服务边界、无工具 run lifecycle、effect-typed tool approval/event records，以及 Artifact / ContextBundle 工作台对象。

当前系统已经可以把 run、tool activity、artifact、context bundle 作为可浏览、可引用、可审计对象保存下来。但这些对象还不能被统一检索，也没有可解释的知识来源、文本分块、记忆 metadata 或 retrieval trace。

Milestone 6 的目标是建立 **围绕 ledger / artifact / context 的可重建检索与知识索引底座**。它不是最终版语义记忆系统，而是为后续 Worker Runtime、embedding、Tantivy、向量库和 agent recall 铺好稳定边界。

## 2. 用户确认范围

采用推荐范围：**SQLite-first 可解释检索 + 索引抽象**。

本 milestone 先做可重建的本地知识与检索基础：Kernel 需要拥有 knowledge source、document chunk、memory metadata、retrieval trace、SearchIndex / VectorIndex 抽象、SQLite read models、HTTP API、TypeScript client 和 Workbench 只读视图。

M6 不直接引入真实 embedding worker，也不把 Tantivy 或任何向量库作为不可替换依赖。全文检索先使用 deterministic SQLite / in-memory 实现，保证可测试、可审计、可重建。

## 3. 目标

1. 引入 `KnowledgeSourceId`、`DocumentChunkId`、`MemoryId`、`RetrievalTraceId` 强类型 ID。
2. 引入 KnowledgeSource 领域对象，用于表示 artifact、context bundle 或手动来源。
3. 引入 DocumentChunk 领域对象，把 artifact/context 内容转成可检索文本块。
4. 引入 MemoryRecord 领域对象，保存可解释记忆 metadata，而不是自动长期记忆推理。
5. 引入 RetrievalTrace 领域对象，记录一次检索请求、命中对象和命中原因。
6. 定义 `SearchIndex` 与 `VectorIndex` 抽象，M6 只提供 deterministic 测试实现和 SQLite-first 关键词检索实现。
7. 提供 additive HTTP API：创建/list knowledge sources、chunks、memories，执行 search，读取 retrieval traces。
8. 提供 TypeScript client 类型与非可选方法。
9. Workbench 增加只读 Knowledge Search、Memory Inspector、Retrieval Traces 视图。
10. 文档明确 M6 与 M7 Worker Runtime、Tantivy、embedding、真实 agent recall 的边界。

## 4. 非目标

Milestone 6 不做：

- 不生成真实 embeddings。
- 不接入真实向量库，例如 LanceDB、Qdrant、sqlite-vec。
- 不正式引入 Tantivy 作为 production full-text sidecar。
- 不做 LLM 自动摘要、自动长期记忆推理或自动遗忘策略。
- 不调用 Worker Runtime；M7 才定义 worker protocol 和执行平面。
- 不做 OCR、PDF parser、网页抓取或 rich document ingestion。
- 不把搜索结果自动注入 agent prompt，也不做 prompt compilation。
- 不让搜索 sidecar 成为唯一事实源；Operation Ledger、SQLite read models 和 artifact content store 仍是权威。
- 不做 cloud sync、多设备同步或个人服务器。

## 5. 架构原则

### 5.1 事实源与 sidecar

Mneme 的事实源仍然是：

1. Operation Ledger events。
2. SQLite read models。
3. Content-addressed artifact store。

Search / Vector / Retrieval ranking 都是可重建 sidecar 或 projection。任何 search index 丢失后，必须能从 SQLite artifacts、context bundles、knowledge sources、document chunks 和 ledger events 重建。

### 5.2 SQLite-first

M6 默认实现选择 SQLite-first：

- knowledge source、chunk、memory、trace metadata 写入 SQLite。
- search MVP 使用确定性关键词匹配，例如 `LIKE`、大小写归一化或 token 包含匹配。
- search 结果必须返回可解释字段：命中 chunk、来源对象、score 或 rank、matched text / reason。

这样 M6 可以在没有 worker、embedding、Tantivy 的情况下提供可用纵向切片。

### 5.3 抽象先行但不过度抽象

M6 定义 `SearchIndex` 与 `VectorIndex` trait，目的是让后续 Tantivy / embedding / vector DB 能替换当前实现。但 M6 不要求抽象覆盖所有未来场景，只覆盖当前服务需要的最小接口。

## 6. 领域模型

### 6.1 ID

新增强类型 ID：

```rust
pub struct KnowledgeSourceId(Uuid);
pub struct DocumentChunkId(Uuid);
pub struct MemoryId(Uuid);
pub struct RetrievalTraceId(Uuid);
```

四者需要与现有 `RunId`、`EventId`、`ArtifactId`、`ContextBundleId` 一致：

- 内部字段私有。
- `new()` 生成 UUID v4。
- `Default` 调用 `new()`。
- `Display` 输出 UUID 字符串。
- `FromStr` 解析 UUID 字符串。
- 支持 serde serialize / deserialize。

### 6.2 KnowledgeSource

KnowledgeSource 是检索系统的来源对象。它不是原始内容本身，而是说明“这些 chunks 或 memories 来自哪里”。

```rust
pub enum KnowledgeSourceKind {
    Artifact,
    ContextBundle,
    Manual,
}

pub struct KnowledgeSourceRecord {
    pub knowledge_source_id: KnowledgeSourceId,
    pub run_id: RunId,
    pub source_kind: KnowledgeSourceKind,
    pub artifact_id: Option<ArtifactId>,
    pub context_bundle_id: Option<ContextBundleId>,
    pub title: String,
    pub summary: String,
    pub created_at: OffsetDateTime,
}
```

约束：

- `Artifact` source 必须引用一个已存在 `ArtifactId`。
- `ContextBundle` source 必须引用一个已存在 `ContextBundleId`。
- `Manual` source 可以不引用 artifact/context bundle。
- `title` 必须非空。
- `summary` 可以是用户提供摘要或 deterministic 摘要，M6 不调用 LLM 自动生成。

### 6.3 DocumentChunk

DocumentChunk 是可检索文本块。

```rust
pub struct DocumentChunkRecord {
    pub document_chunk_id: DocumentChunkId,
    pub knowledge_source_id: KnowledgeSourceId,
    pub run_id: RunId,
    pub sequence: i64,
    pub text: String,
    pub char_start: i64,
    pub char_end: i64,
    pub created_at: OffsetDateTime,
}
```

M6 chunking 策略：

- Text / Markdown / Json artifact 可以被 chunk。
- Binary artifact 不生成 chunk，只能保留 source metadata。
- 默认 deterministic chunking：按字符或行切分，目标大小约 800–1200 字符，允许 overlap 100–200 字符。
- `sequence` 从 0 开始递增。
- `char_start` / `char_end` 用于解释命中位置，不要求支持 Unicode grapheme 级精确编辑。

### 6.4 MemoryRecord

MemoryRecord 是可解释记忆 metadata。M6 只支持手动或显式 API 创建，不自动从对话中抽取长期记忆。

```rust
pub enum MemoryKind {
    Fact,
    Decision,
    Preference,
    Insight,
}

pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub knowledge_source_id: Option<KnowledgeSourceId>,
    pub run_id: RunId,
    pub kind: MemoryKind,
    pub title: String,
    pub body: String,
    pub confidence: f32,
    pub created_at: OffsetDateTime,
}
```

约束：

- `title` 与 `body` 必须非空。
- `confidence` 范围是 `0.0..=1.0`。
- `knowledge_source_id` 可选，用于连接记忆与来源。
- `run_id` 必须存在，因为当前 `OperationEvent` 结构强制关联 run；M6 不扩大 ledger 基础结构。
- M6 不做自动重要性评分、遗忘、冲突检测或语义聚类。

### 6.5 RetrievalTrace

RetrievalTrace 记录一次检索或召回请求及其结果解释。

```rust
pub struct RetrievalTraceRecord {
    pub retrieval_trace_id: RetrievalTraceId,
    pub run_id: RunId,
    pub query: String,
    pub result_chunk_ids: Vec<DocumentChunkId>,
    pub result_memory_ids: Vec<MemoryId>,
    pub explanation: String,
    pub created_at: OffsetDateTime,
}
```

约束：

- `query` 必须非空。
- `explanation` 必须说明 M6 使用的是 deterministic keyword retrieval，而不是 semantic embedding。
- trace 是审计记录，不代表 agent 已自动使用这些结果。

## 7. OperationEvent 扩展

M6 新增事件类型：

```rust
pub enum OperationEventKind {
    KnowledgeSourceCreated {
        knowledge_source_id: KnowledgeSourceId,
        source_kind: KnowledgeSourceKind,
        artifact_id: Option<ArtifactId>,
        context_bundle_id: Option<ContextBundleId>,
        title: String,
        summary: String,
    },
    DocumentChunkCreated {
        document_chunk_id: DocumentChunkId,
        knowledge_source_id: KnowledgeSourceId,
        sequence: i64,
        text: String,
        char_start: i64,
        char_end: i64,
    },
    MemoryRecorded {
        memory_id: MemoryId,
        knowledge_source_id: Option<KnowledgeSourceId>,
        kind: MemoryKind,
        title: String,
        body: String,
        confidence: f32,
    },
    RetrievalTraceRecorded {
        retrieval_trace_id: RetrievalTraceId,
        query: String,
        result_chunk_ids: Vec<DocumentChunkId>,
        result_memory_ids: Vec<MemoryId>,
        explanation: String,
    },
}
```

事件构造函数：

```rust
OperationEvent::new_knowledge_source_created(...)
OperationEvent::new_document_chunk_created(...)
OperationEvent::new_memory_recorded(...)
OperationEvent::new_retrieval_trace_recorded(...)
```

所有事件继续包含 `event_id`、`run_id`、`occurred_at`。当前 `OperationEvent` 强制 `run_id: RunId`，所以 M6 所有 knowledge / memory / retrieval 操作都必须关联一个 run。M6 不引入全局无 run 事件，避免扩大 ledger 基础结构。

## 8. SQLite read models

新增表：

```sql
create table knowledge_sources (
  knowledge_source_id text primary key,
  run_id text not null,
  source_kind text not null,
  artifact_id text,
  context_bundle_id text,
  title text not null,
  summary text not null,
  created_at text not null
);

create index idx_knowledge_sources_run_id_created_at
  on knowledge_sources(run_id, created_at);

create table document_chunks (
  document_chunk_id text primary key,
  knowledge_source_id text not null,
  run_id text not null,
  sequence integer not null,
  text text not null,
  char_start integer not null,
  char_end integer not null,
  created_at text not null
);

create index idx_document_chunks_source_sequence
  on document_chunks(knowledge_source_id, sequence);

create table memory_records (
  memory_id text primary key,
  knowledge_source_id text,
  run_id text not null,
  kind text not null,
  title text not null,
  body text not null,
  confidence real not null,
  created_at text not null
);

create index idx_memory_records_source_created_at
  on memory_records(knowledge_source_id, created_at);

create table retrieval_traces (
  retrieval_trace_id text primary key,
  run_id text not null,
  query text not null,
  result_chunk_ids_json text not null,
  result_memory_ids_json text not null,
  explanation text not null,
  created_at text not null
);

create index idx_retrieval_traces_run_id_created_at
  on retrieval_traces(run_id, created_at);
```

所有 projection 必须支持从 ledger events rebuild。Search sidecar 如果存在，也必须能从这些 read models rebuild。

## 9. Kernel service 边界

### 9.1 KnowledgeService

```rust
pub trait KnowledgeService: Send + Sync {
    async fn create_source_from_artifact(
        &self,
        artifact_id: ArtifactId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError>;

    async fn create_source_from_context_bundle(
        &self,
        context_bundle_id: ContextBundleId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError>;

    async fn create_manual_source(
        &self,
        run_id: RunId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError>;

    async fn list_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError>;
}
```

### 9.2 ChunkService

```rust
pub trait ChunkService: Send + Sync {
    async fn chunk_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError>;

    async fn list_chunks(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError>;
}
```

### 9.3 MemoryService

```rust
pub trait MemoryService: Send + Sync {
    async fn record_memory(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
        kind: MemoryKind,
        title: String,
        body: String,
        confidence: f32,
    ) -> Result<RecordMemoryOutcome, MemoryServiceError>;

    async fn list_memories(&self) -> Result<Vec<MemoryRecord>, MemoryServiceError>;
}
```

### 9.4 RetrievalService

```rust
pub trait RetrievalService: Send + Sync {
    async fn search(
        &self,
        query: String,
        limit: usize,
    ) -> Result<SearchResponse, RetrievalServiceError>;

    async fn list_retrieval_traces(
        &self,
    ) -> Result<Vec<RetrievalTraceRecord>, RetrievalServiceError>;
}
```

服务错误要区分：

- invalid input。
- unknown source / artifact / context bundle / chunk / memory。
- ledger failure。
- projection failure。
- index failure。

## 10. SearchIndex 与 VectorIndex

### 10.1 SearchIndex

```rust
pub trait SearchIndex: Send + Sync {
    async fn rebuild(&self, chunks: Vec<DocumentChunkRecord>) -> Result<(), SearchIndexError>;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, SearchIndexError>;
}

pub struct SearchHit {
    pub document_chunk_id: DocumentChunkId,
    pub score: f32,
    pub reason: String,
}
```

M6 实现：

- `InMemorySearchIndex`：用于 unit tests。
- `SqliteKeywordSearchIndex` 或等价服务内实现：使用 document_chunks 表做 deterministic keyword search。

### 10.2 VectorIndex

```rust
pub trait VectorIndex: Send + Sync {
    async fn upsert(&self, chunk_id: DocumentChunkId, embedding: Vec<f32>) -> Result<(), VectorIndexError>;
    async fn search(&self, embedding: Vec<f32>, limit: usize) -> Result<Vec<VectorHit>, VectorIndexError>;
}
```

M6 只定义 trait 和 `NoopVectorIndex` / `InMemoryVectorIndex` 测试 stub。它不能暗示系统已经具备 semantic search。

## 11. HTTP API

新增 routes：

```text
GET  /api/v1/knowledge-sources
POST /api/v1/artifacts/:artifact_id/knowledge-source
POST /api/v1/context-bundles/:context_bundle_id/knowledge-source
POST /api/v1/knowledge-sources/manual

GET  /api/v1/document-chunks
POST /api/v1/knowledge-sources/:knowledge_source_id/chunks

GET  /api/v1/memories
POST /api/v1/memories

POST /api/v1/search
GET  /api/v1/retrieval-traces
```

DTOs：

- `CreateKnowledgeSourceRequest { title, summary }`
- `CreateManualKnowledgeSourceRequest { run_id, title, summary }`
- `CreateKnowledgeSourceResponse { knowledge_source_id }`
- `KnowledgeSourcesResponse { knowledge_sources }`
- `DocumentChunksResponse { document_chunks }`
- `RecordMemoryRequest { knowledge_source_id?, kind, title, body, confidence }`
- `RecordMemoryResponse { memory_id }`
- `MemoriesResponse { memories }`
- `SearchRequest { query, limit? }`
- `SearchResponse { query, hits, retrieval_trace_id }`
- `RetrievalTracesResponse { retrieval_traces }`

错误映射：

- invalid UUID / blank input / invalid confidence / invalid limit → `400 BAD_REQUEST`。
- unknown artifact / context bundle / knowledge source / chunk / memory → `404 NOT_FOUND`。
- ledger / projection / index / storage failure → `500 INTERNAL_SERVER_ERROR`。

## 12. TypeScript client

新增类型：

```ts
export type KnowledgeSourceKind = 'artifact' | 'context_bundle' | 'manual'
export type MemoryKind = 'fact' | 'decision' | 'preference' | 'insight'
export interface KnowledgeSourceRecord { ... }
export interface DocumentChunkRecord { ... }
export interface MemoryRecord { ... }
export interface RetrievalTraceRecord { ... }
export interface SearchHit { ... }
```

`MnemeApiClient` 新增非可选方法：

```ts
listKnowledgeSources(): Promise<KnowledgeSourcesResponse>
createKnowledgeSourceFromArtifact(artifactId: string, request: CreateKnowledgeSourceRequest): Promise<CreateKnowledgeSourceResponse>
createKnowledgeSourceFromContextBundle(contextBundleId: string, request: CreateKnowledgeSourceRequest): Promise<CreateKnowledgeSourceResponse>
createManualKnowledgeSource(request: CreateManualKnowledgeSourceRequest): Promise<CreateKnowledgeSourceResponse>
listDocumentChunks(): Promise<DocumentChunksResponse>
chunkKnowledgeSource(knowledgeSourceId: string): Promise<DocumentChunksResponse>
listMemories(): Promise<MemoriesResponse>
recordMemory(request: RecordMemoryRequest): Promise<RecordMemoryResponse>
search(request: SearchRequest): Promise<SearchResponse>
listRetrievalTraces(): Promise<RetrievalTracesResponse>
```

## 13. Workbench UI

Workbench 新增三个只读 section，放在 Artifact Workspace / Context Inspector 之后：

### 13.1 Knowledge Search

展示：

- search input。
- search results。
- hit chunk text。
- source title/kind。
- score / reason。
- retrieval trace id。

UI 必须明确提示：M6 search 是 deterministic keyword retrieval，不是 semantic embedding search。

### 13.2 Memory Inspector

展示：

- memory title。
- kind。
- body preview。
- confidence。
- linked knowledge source。

M6 可以提供 record memory 的 API，但 UI 默认先做只读列表。是否加入简单 create form 可以在 implementation plan 中拆成独立任务判断，不能阻碍核心检索链路。

### 13.3 Retrieval Traces

展示：

- query。
- result chunk count。
- result memory count。
- explanation。
- created_at。

不展示 agent 已使用结果的暗示。

## 14. 与后续 milestone 的 handoff

### 14.1 Milestone 7 Worker Runtime

M7 可以在 M6 的基础上新增：

- embedding worker。
- document parser / OCR worker。
- LLM summary worker。
- async reindex job。

M7 worker 应调用 M6 service 边界写入 chunks、memories、retrieval traces，而不是直接写 SQLite。

### 14.2 Tantivy / Vector sidecar

后续可以新增：

- `TantivySearchIndex` 实现 `SearchIndex`。
- `sqlite-vec` / LanceDB / Qdrant 实现 `VectorIndex`。
- sidecar rebuild command。

这些不得改变 M6 ledger facts。

### 14.3 Agent recall

后续 agent loop 可以调用 `RetrievalService::search()`，并把 retrieval trace event 写入 ledger。M6 只提供可解释检索，不自动改变 agent 行为。

## 15. 测试策略

Kernel tests：

- ID 强类型、Display、FromStr、serde roundtrip。
- KnowledgeSource / DocumentChunk / MemoryRecord / RetrievalTrace serde roundtrip。
- OperationEvent constructors and SQLite ledger roundtrip。
- In-memory projection apply/list/get。
- SQLite projection rebuild from ledger events。
- Chunking deterministic behavior。
- SearchIndex deterministic keyword hit ordering。
- RetrievalService records RetrievalTrace.

API tests：

- create/list knowledge source。
- chunk source and list chunks。
- record/list memory。
- search returns hits and trace id。
- list retrieval traces。
- invalid UUID / blank query / invalid confidence / unknown source mappings。

TS tests：

- each client method uses correct HTTP method/path/body。
- App test double includes all new non-optional methods。

UI tests：

- Knowledge Search renders result source/chunk/reason。
- Memory Inspector renders memory metadata。
- Retrieval Traces renders query/explanation/counts。
- UI copy does not claim semantic embedding or real agent recall.

Docs checks：

- M6 architecture doc explains SQLite-first, sidecar rebuildability, non-goals, M7 handoff。
- README includes minimal curl examples for source/chunk/search/memory/trace。

Final verification：

```bash
. "$HOME/.cargo/env" && pnpm check
git diff --check
git status --short --branch
```

## 16. 实施任务建议

1. Kernel IDs、domain records、event variants。
2. Knowledge source projection/service + SQLite。
3. Document chunking projection/service + SQLite。
4. Memory record projection/service + SQLite。
5. SearchIndex / VectorIndex abstractions and deterministic search implementation。
6. RetrievalService and retrieval trace persistence。
7. HTTP API routes and daemon wiring。
8. TypeScript client methods。
9. Workbench Knowledge Search / Memory Inspector / Retrieval Traces UI。
10. Docs and final verification / broad review。

## 17. 成功标准

Milestone 6 完成时必须满足：

1. Kernel 导出 knowledge/search/memory/retrieval 类型与服务。
2. Operation Ledger 能序列化、持久化并回放 M6 events。
3. SQLite read models 能从 ledger events rebuild。
4. Artifact/context 对象能被提升为 knowledge source 并生成 deterministic chunks。
5. Search API 能返回可解释 hits，并记录 retrieval trace。
6. Memory metadata 可以显式创建、列出、关联 source。
7. SearchIndex / VectorIndex trait 存在，M6 有可测试 deterministic implementation / stub。
8. HTTP API、TS client、Workbench 形成端到端纵向切片。
9. UI 和 docs 不声称已具备 semantic embedding、Tantivy production index、自动长期记忆或 agent recall。
10. `. "$HOME/.cargo/env" && pnpm check`、`git diff --check`、最终 review 全部通过。
