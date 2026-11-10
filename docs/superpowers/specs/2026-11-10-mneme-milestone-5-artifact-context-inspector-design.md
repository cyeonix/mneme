# Mneme Milestone 5：Artifact 与 Context Inspector 设计

## 1. 背景

Milestone 1–4 已经完成 Mneme Phase 1 的本地持久化数据面、Kernel 服务边界、无工具 run lifecycle，以及 effect-typed tool approval/event records。当前 Workbench 已能展示 runs 与 tool activity，但 run 产物、上下文引用和可解释材料仍停留在事件文本中，无法作为一等对象浏览、引用或交给后续搜索/记忆层使用。

Milestone 5 的目标是把 **run 输出、上下文、附件** 提升为可浏览、可引用、可审计的 Kernel 对象：Artifact 与 ContextBundle。

## 2. 用户确认范围

采用推荐范围：**本地 content-addressed Artifact Store + ContextBundle 只读 Inspector**。

本 milestone 要做真实的纵向切片，而不是 UI-only mock：Kernel 需要拥有 artifact/context 领域类型、ledger events、SQLite 投影、API DTO/routes 和 Workbench 只读展示。

## 3. 目标

1. 引入 `ArtifactId` 与 `ContextBundleId` 强类型 ID。
2. 引入 Artifact 领域对象、事件和可查询投影。
3. 引入本地 content-addressed artifact store，用 `sha256` 作为内容地址。
4. 引入 ContextBundle 领域对象、事件和可查询投影，用于解释一个 run 的上下文由哪些 events/artifacts/notes 组成。
5. 提供 additive HTTP API：创建/列表/详情读取 artifacts 和 context bundles。
6. 提供 TypeScript client 类型与方法。
7. Workbench 增加只读 Artifact Workspace 与 Context Inspector。
8. 文档明确 M5 边界和后续 M6/M7 handoff。

## 4. 非目标

Milestone 5 不做：

- 不做全文搜索、向量索引、记忆抽取或知识库 ranking；这些属于 Milestone 6。
- 不做 LLM prompt compilation；ContextBundle 只是可解释引用集合，不是最终 prompt。
- 不执行真实工具，不接 MCP / OpenAPI / CLI / local script；真实 worker/tool execution 属于后续 milestone。
- 不做 Tauri 打包，不做 OS 文件选择器，不做权限弹窗。
- 不做云同步、多设备同步或个人服务器。
- 不做图片、音频、视频等二进制富预览；binary artifact 只保存 metadata 与内容地址。
- 不让 API 或 UI 直接写 SQLite 或绕过 Kernel service/storage 边界。

## 5. 领域模型

### 5.1 ID

新增强类型 ID：

```rust
pub struct ArtifactId(Uuid);
pub struct ContextBundleId(Uuid);
```

两者必须与 `EventId`、`RunId`、`ToolCallId` 一样：

- 内部字段私有。
- `new()` 生成 UUID v4。
- `Default` 调用 `new()`。
- `Display` 输出 UUID 字符串。
- `FromStr` 解析 UUID 字符串。
- 支持 serde serialize/deserialize。

### 5.2 Artifact

Artifact 是 run 产生或用户/API 记录的可引用材料。

```rust
pub enum ArtifactKind {
    Text,
    Markdown,
    Json,
    Binary,
}

pub struct ArtifactRecord {
    pub artifact_id: ArtifactId,
    pub run_id: RunId,
    pub title: String,
    pub kind: ArtifactKind,
    pub mime_type: String,
    pub sha256: String,
    pub byte_len: u64,
    pub created_at: OffsetDateTime,
}
```

Artifact content 由 content-addressed store 保存。`ArtifactRecord` 只保存 metadata 与 content hash。

### 5.3 ContextBundle

ContextBundle 是可解释的上下文引用集合。它不是 LLM prompt，也不做自动摘要生成。

```rust
pub struct ContextBundleRecord {
    pub context_bundle_id: ContextBundleId,
    pub run_id: RunId,
    pub title: String,
    pub summary: String,
    pub artifact_ids: Vec<ArtifactId>,
    pub event_ids: Vec<EventId>,
    pub created_at: OffsetDateTime,
}
```

M5 中 ContextBundle 的用途：

- 解释某次 run 的上下文材料从哪里来。
- 让 Workbench 展示 artifact/event 引用关系。
- 为 Milestone 6 搜索/记忆和 Milestone 7 worker runtime 提供稳定输入对象。

## 6. Operation Ledger Events

`OperationEventKind` 新增：

```rust
ArtifactCreated {
    artifact_id: ArtifactId,
    title: String,
    kind: ArtifactKind,
    mime_type: String,
    sha256: String,
    byte_len: u64,
}

ContextBundleCreated {
    context_bundle_id: ContextBundleId,
    title: String,
    summary: String,
    artifact_ids: Vec<ArtifactId>,
    event_ids: Vec<EventId>,
}
```

构造函数：

```rust
OperationEvent::new_artifact_created(...)
OperationEvent::new_context_bundle_created(...)
```

事件保持 append-only。Artifact/ContextBundle 投影必须能从 ledger replay 得到确定性结果。

## 7. Storage 与 Projection

### 7.1 SQLite tables

在现有 migration 基础上增加投影表：

```sql
create table if not exists artifacts (
  artifact_id text primary key,
  run_id text not null,
  title text not null,
  kind text not null,
  mime_type text not null,
  sha256 text not null,
  byte_len integer not null,
  created_at text not null
);

create index if not exists idx_artifacts_run_id_created_at
  on artifacts(run_id, created_at desc);

create table if not exists context_bundles (
  context_bundle_id text primary key,
  run_id text not null,
  title text not null,
  summary text not null,
  artifact_ids_json text not null,
  event_ids_json text not null,
  created_at text not null
);

create index if not exists idx_context_bundles_run_id_created_at
  on context_bundles(run_id, created_at desc);
```

### 7.2 Artifact content-addressed store

新增本地 artifact content store：

```rust
pub trait ArtifactContentStore: Send + Sync {
    async fn put(&self, content: Vec<u8>) -> Result<StoredArtifactContent, ArtifactStoreError>;
    async fn get(&self, sha256: &str) -> Result<Vec<u8>, ArtifactStoreError>;
}

pub struct StoredArtifactContent {
    pub sha256: String,
    pub byte_len: u64,
}
```

推荐路径策略：

```text
<mneme-data-dir>/artifacts/sha256/<sha256>
```

M5 可以实现：

- `InMemoryArtifactContentStore`：用于测试。
- `LocalArtifactContentStore`：用于 daemon 默认本地数据目录。

写入同一内容必须得到相同 `sha256`。重复写入同一 hash 不应破坏已有文件。

### 7.3 Projection stores

新增：

```rust
pub trait ArtifactProjectionStore
pub trait ContextBundleProjectionStore
```

职责：

- 应用 `ArtifactCreated` / `ContextBundleCreated` event。
- 列表查询。
- 按 ID 详情查询。
- 可从 ledger replay 重建。

## 8. Service Boundary

M5 推荐新增独立服务，而不是继续扩大 `RunService`：

```rust
pub trait ArtifactService: Send + Sync {
    async fn create_text_artifact(
        &self,
        run_id: RunId,
        title: String,
        kind: ArtifactKind,
        mime_type: String,
        content: String,
    ) -> Result<CreateArtifactOutcome, ArtifactServiceError>;

    async fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>, ArtifactServiceError>;
    async fn get_artifact(&self, artifact_id: ArtifactId) -> Result<ArtifactDetail, ArtifactServiceError>;
}

pub trait ContextService: Send + Sync {
    async fn create_context_bundle(
        &self,
        run_id: RunId,
        title: String,
        summary: String,
        artifact_ids: Vec<ArtifactId>,
        event_ids: Vec<EventId>,
    ) -> Result<CreateContextBundleOutcome, ContextServiceError>;

    async fn list_context_bundles(&self) -> Result<Vec<ContextBundleRecord>, ContextServiceError>;
    async fn get_context_bundle(
        &self,
        context_bundle_id: ContextBundleId,
    ) -> Result<ContextBundleRecord, ContextServiceError>;
}
```

Service 必须负责：

- trim/validate blank title/summary/content。
- 校验 `run_id` 已知。
- Artifact 写入 content store 后追加 ledger event 并更新投影。
- ContextBundle 追加 ledger event 并更新投影。
- 不允许 API route 直接写 ledger/projection/content store。

M5 可以不强制 terminal run 拒绝 artifact/context 创建。理由：artifact/context 可能在 run 完成后作为结果整理或人工记录产生。但服务必须校验 run 存在。

## 9. HTTP API

新增 additive endpoints：

```text
GET  /api/v1/artifacts
POST /api/v1/runs/:run_id/artifacts
GET  /api/v1/artifacts/:artifact_id

GET  /api/v1/context-bundles
POST /api/v1/runs/:run_id/context-bundles
GET  /api/v1/context-bundles/:context_bundle_id
```

DTO 示例：

```json
{
  "title": "Run summary",
  "kind": "markdown",
  "mime_type": "text/markdown",
  "content": "# Summary\n..."
}
```

ContextBundle create：

```json
{
  "title": "Debug context",
  "summary": "用户问题、相关输出和工具结果。",
  "artifact_ids": ["..."],
  "event_ids": ["..."]
}
```

错误映射：

- invalid UUID → `400 BAD_REQUEST`
- blank required text → `400 BAD_REQUEST`
- unknown run/artifact/context → `404 NOT_FOUND` 或 `400 BAD_REQUEST`，计划中需固定一种；推荐 unknown resource 用 `404 NOT_FOUND`
- storage/ledger/projection/content-store error → `500 INTERNAL_SERVER_ERROR`

## 10. TypeScript Client

新增类型：

```ts
export type ArtifactKind = 'text' | 'markdown' | 'json' | 'binary';

export interface ArtifactRecord { ... }
export interface ArtifactDetail { artifact: ArtifactRecord; content: string | null; }
export interface ContextBundleRecord { ... }
```

新增方法：

```ts
listArtifacts(): Promise<ArtifactsResponse>;
createArtifact(runId: string, request: CreateArtifactRequest): Promise<CreateArtifactResponse>;
getArtifact(artifactId: string): Promise<ArtifactDetail>;
listContextBundles(): Promise<ContextBundlesResponse>;
createContextBundle(runId: string, request: CreateContextBundleRequest): Promise<CreateContextBundleResponse>;
getContextBundle(contextBundleId: string): Promise<ContextBundleRecord>;
```

## 11. Workbench UI

Workbench 增加两个只读区块：

### 11.1 Artifact Workspace

展示：

- title
- kind / mime type
- byte length
- sha256 前 12–16 位
- run id
- text/markdown/json content preview（截断显示）

空状态说明：

> 还没有 artifacts。Milestone 5 会把 run 输出和上下文材料保存为可引用对象。

### 11.2 Context Inspector

展示：

- title
- summary
- run id
- artifact count
- event count
- created_at

空状态说明：

> 还没有 context bundles。Context Inspector 用来解释一次 run 使用了哪些 artifacts 和 ledger events。

UI 不提供复杂编辑器、不做拖拽、不做搜索、不做 prompt compile。

## 12. Daemon 与本地数据目录

Daemon 应继续默认使用本地 SQLite 数据库，并为 artifact content store 提供稳定路径。

建议：

```text
MNEME_DATA_DIR unset:
  ~/.local/share/mneme/

SQLite:
  ~/.local/share/mneme/mneme.sqlite3

Artifacts:
  ~/.local/share/mneme/artifacts/sha256/<sha256>
```

如果当前 daemon 已有数据库路径推导逻辑，M5 不应重写，只在同一数据目录下派生 artifact store 路径。

## 13. 测试策略

每层至少覆盖：

1. ID newtype display/from_str。
2. `ArtifactCreated` / `ContextBundleCreated` event serde roundtrip。
3. content store sha256/byte_len/dedup 行为。
4. in-memory service create/list/get。
5. SQLite service create/list/get + ledger event + projection row 一致性。
6. API create/list/get happy path 与 invalid UUID/blank text/unknown resource。
7. TS client endpoint paths/body。
8. Workbench renders Artifact Workspace and Context Inspector read-only sections。
9. Final `pnpm check` and `git diff --check`。

## 14. 实施任务建议

1. Kernel IDs、Artifact/Context domain types 与 events。
2. Content-addressed artifact store。
3. Artifact projection/service + SQLite integration。
4. ContextBundle projection/service + SQLite integration。
5. HTTP API DTO/routes。
6. TypeScript client。
7. Workbench Artifact Workspace + Context Inspector。
8. Documentation。
9. Final verification + broad review。

## 15. 成功标准

Milestone 5 完成时必须满足：

- Kernel 导出 `ArtifactId`、`ContextBundleId`、`ArtifactKind`、`ArtifactRecord`、`ContextBundleRecord`。
- Operation Ledger 能持久化/反序列化 artifact/context events。
- 本地 content-addressed store 能以 sha256 保存和读取 artifact content。
- SQLite 中 artifacts/context_bundles 投影可从服务写入并查询。
- API、TS client、Workbench 都能围绕 artifact/context 工作。
- Workbench 明确是只读浏览，不声称搜索、记忆、真实工具执行或 prompt compile。
- 文档说明 M5 和 M6/M7 的边界。
- `. "$HOME/.cargo/env" && pnpm check` 通过。
- `git diff --check` 通过。

## 16. 自检

- Scope：本设计只覆盖 Artifact Store 与 Context Inspector，不进入 Search/Memory/Knowledge、Worker Runtime、Tauri 或云同步。
- Architecture：沿用 Rust Kernel control plane、Operation Ledger fact source、SQLite projection、React/Vite Workbench。
- Ambiguity：ContextBundle 被定义为引用集合，不是 prompt；Artifact content store 被定义为 content-addressed 本地文件存储。
- API：全部 additive，不破坏现有 health/events/runs/lifecycle/tool endpoints。
- UI：只读展示，不添加真实执行能力。
