# Phase 1 Foundation Architecture

Phase 1 验证 Mneme 的第一条纵向切片：本地 Kernel、Operation Ledger、SQLite 读模型、HTTP API、typed client 与最小 Workbench。它仍不包含真实模型调用或 worker runtime。

## 边界

- `mneme-kernel` 拥有 run/tool/artifact/context 领域类型、Operation Ledger 接口和 service boundary。
- `mneme-api` 暴露 HTTP DTO 与 routes。
- `mneme-daemon` 负责进程启动和本地监听地址。
- `apps/desktop` 通过 typed API client 消费 Kernel API，不直接写 Kernel 状态。

## 数据流

```text
React Workbench
  → MnemeApiClient
  → HTTP /api/v1/*
  → mneme-api routes
  → mneme-kernel ledger
```

## 存储演进

Phase 1 初始切片使用 in-memory ledger，以便先确认事件 schema 与 API 边界。Milestone 1/2 之后，daemon 默认使用 SQLite ledger、runs 投影和 Kernel service data plane；内存实现仅作为早期上下文和测试替身。

Milestone 5 在同一条 Operation Ledger 上增加 artifact/context 事件，并新增 `artifacts`、`context_bundles` SQLite 读模型。Artifact 内容按 SHA-256 寻址，Operation Ledger 只记录 content address 与 metadata；context bundle 只记录 artifact/event 引用。

Milestone 6 增加 SQLite-first knowledge sources、document chunks、memory metadata、deterministic retrieval 与 retrieval traces，对应 `knowledge_sources`、`document_chunks`、`memory_records`、`retrieval_traces` 读模型；M6 只提供可解释关键字检索和只读 Workbench inspector，M7 才负责 worker/embedding execution。

## Phase 1 之后的数据面演进

Milestone 1 在不改变外部 API 基本形状的前提下，把内存 ledger 替换为 SQLite ledger，并加入 runs 投影。Phase 1 的 API route、daemon、TypeScript client 和 Workbench 会继续存在，只是数据来源从内存对象变成可持久化 Kernel 数据面。

Milestone 2 进一步把创建 run 的业务流程收敛进 Kernel service，避免 HTTP route 直接组合多个存储写入。

Milestone 4 增加 effect-typed tool approval event 记录，同时保持 daemon/API adapters 位于 `RunService` 之后。

Milestone 5 增加 artifact/context IDs、`ArtifactCreated` 与 `ContextBundleCreated` 事件、content-addressed artifact store、SQLite artifact/context 读模型，以及 read-only Workbench `Artifact Workspace` 和 `Context Inspector`。它不实现搜索、向量召回、memory ranking、真实 worker runtime、真实模型/工具执行、Tauri/cloud sync，或编辑/搜索/拖拽/投递 prompt compile UI。

Milestone 6 在 M5 的 artifact/context 之上增加 `KnowledgeSourceCreated`、`DocumentChunkCreated`、`MemoryRecorded`、`RetrievalTraceRecorded` 事件和 Search / Memory / Knowledge inspector。检索路径是 SQLite-first deterministic keyword retrieval：从 `document_chunks` 做关键字匹配，join `knowledge_sources` 返回来源 metadata，并把每次搜索的 explanation 写入 `retrieval_traces`。它不宣称 real embeddings、Tantivy production index、vector DB、Worker Runtime、OCR/PDF/web ingestion、automatic long-term memory、automatic agent recall 或 prompt injection 已实现；这些由 M7 及之后的 worker/embedding execution 继续推进。

## 验证

```bash
pnpm check
```
