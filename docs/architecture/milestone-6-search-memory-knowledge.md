# Milestone 6：Search / Memory / Knowledge

Milestone 6 在 Milestone 5 的 artifact/context 基础上，增加 SQLite-first 的可解释检索、知识来源、文档分块、记忆元数据和检索轨迹。它的目标是让本地 Workbench 与 HTTP API 可以观察「哪些文本被纳入知识面、搜索为什么命中、检索动作如何写回 Operation Ledger」，而不是交付 embedding 或 worker 执行层。

## Scope

包含：

- 新增领域 ID：`KnowledgeSourceId`、`DocumentChunkId`、`MemoryId`、`RetrievalTraceId`。
- 新增领域模型：`KnowledgeSourceRecord`、`DocumentChunkRecord`、`MemoryRecord`、`RetrievalTraceRecord`、`SearchResult`、`SearchResponse`。
- 新增枚举：`KnowledgeSourceKind::{Artifact, ContextBundle, Manual}` 与 `MemoryKind::{Fact, Decision, Preference, Insight}`。
- 新增 Operation Ledger 事件：`KnowledgeSourceCreated`、`DocumentChunkCreated`、`MemoryRecorded`、`RetrievalTraceRecorded`。
- 新增 SQLite 读模型表：`knowledge_sources`、`document_chunks`、`memory_records`、`retrieval_traces`。
- 新增索引：`idx_knowledge_sources_run_id_created_at`、`idx_document_chunks_source_sequence`、`idx_memory_records_source_created_at`、`idx_retrieval_traces_run_id_created_at`。
- 新增 HTTP endpoints 与 TypeScript client 方法，用于创建知识来源、生成 chunk、记录 memory、执行 deterministic keyword search、查看 retrieval trace。
- Workbench 新增只读 Search / Memory / Knowledge inspector，用于展示知识来源、chunk、memory 和检索轨迹。

M6 延续 Phase 1 的 service boundary：HTTP route 只调用 Kernel service；SQLite service 在事务内追加 `operation_events` 并更新读模型；`operation_events` 仍是可重放事实源。

## Event Flow

M6 的概念流如下：

```text
Run
  → Artifact / ContextBundle / Manual metadata
  → KnowledgeSourceCreated
  → DocumentChunkCreated
  → MemoryRecorded
  → RetrievalTraceRecorded
```

Artifact 来源知识：

```text
POST /api/v1/artifacts/:artifact_id/knowledge-source
  → validate artifact_id
  → infer run_id from artifacts
  → OperationEventKind::KnowledgeSourceCreated { source_kind: Artifact }
  → operation_events append
  → knowledge_sources projection upsert
```

Context bundle 来源知识：

```text
POST /api/v1/context-bundles/:context_bundle_id/knowledge-source
  → validate context_bundle_id
  → infer run_id from context_bundles
  → OperationEventKind::KnowledgeSourceCreated { source_kind: ContextBundle }
  → operation_events append
  → knowledge_sources projection upsert
```

Manual 来源知识：

```text
POST /api/v1/knowledge-sources/manual
  → validate run_id, title, summary
  → OperationEventKind::KnowledgeSourceCreated { source_kind: Manual }
  → operation_events append
  → knowledge_sources projection upsert
```

Chunk 生成：

```text
POST /api/v1/knowledge-sources/:knowledge_source_id/chunks
  → load knowledge source summary
  → ChunkTextSplitter { target_chars: 240, overlap_chars: 40 }
  → OperationEventKind::DocumentChunkCreated for each chunk
  → operation_events append
  → document_chunks projection upsert
```

Memory 记录：

```text
POST /api/v1/memories
  → validate run_id, optional knowledge_source_id, kind, title, body, confidence
  → OperationEventKind::MemoryRecorded
  → operation_events append
  → memory_records projection upsert
```

Search 与 trace：

```text
POST /api/v1/search
  → validate run_id, query, limit
  → deterministic keyword match over document_chunks.text
  → build SearchResult { chunk, source, score, reason }
  → OperationEventKind::RetrievalTraceRecorded
  → operation_events append
  → retrieval_traces projection upsert
```

`RetrievalTraceRecorded` 保存 `query`、`result_chunk_ids`、`result_memory_ids` 和 `explanation`。当前 SQLite search 写入 chunk 命中列表，`result_memory_ids` 保留为空列表，作为后续 memory ranking 或 embedding 检索的兼容字段。

## SQLite-first Retrieval

M6 的检索路径刻意采用 SQLite-first deterministic keyword retrieval：

- 搜索直接查询 `document_chunks`，并通过 `knowledge_sources` join 补充来源 metadata。
- SQLite 查询使用 `lower(c.text) like '%' || lower(?1) || '%'` 做大小写无关关键字匹配。
- 结果按 `knowledge_source_id asc, sequence asc` 排序，再应用请求的 `limit`。
- 每个命中返回固定解释：`keyword '<query>' matched chunk text`。
- 每次搜索都会写入一条 `RetrievalTraceRecorded` 事件，并投影到 `retrieval_traces`。
- `retrieval_traces.explanation` 使用可读文本说明 M6 deterministic keyword retrieval 命中了多少 chunk。

SQLite 表结构：

- `knowledge_sources`：保存 `knowledge_source_id`、`run_id`、`source_kind`、可选 `artifact_id` / `context_bundle_id`、`title`、`summary`、`created_at`。
- `document_chunks`：保存 `document_chunk_id`、`knowledge_source_id`、`run_id`、`sequence`、`text`、`char_start`、`char_end`、`created_at`。
- `memory_records`：保存 `memory_id`、可选 `knowledge_source_id`、`run_id`、`kind`、`title`、`body`、`confidence`、`created_at`。
- `retrieval_traces`：保存 `retrieval_trace_id`、`run_id`、`query`、`result_chunk_ids_json`、`result_memory_ids_json`、`explanation`、`created_at`。

Search index 与 vector index 在 M6 只是 sidecar 抽象边界。SQLite 读模型可由 ledger event 重建，因此这些 sidecar 可以从 `operation_events` 与 SQLite projections 重新生成；M6 默认运行路径不依赖外部索引服务。

## HTTP API

Knowledge source endpoints：

- `GET /api/v1/knowledge-sources`：列出全部知识来源。
- `POST /api/v1/artifacts/:artifact_id/knowledge-source`：从已有 artifact metadata 创建知识来源。
- `POST /api/v1/context-bundles/:context_bundle_id/knowledge-source`：从已有 context bundle metadata 创建知识来源。
- `POST /api/v1/knowledge-sources/manual`：从手动输入的 `run_id`、`title`、`summary` 创建知识来源。

Document chunk endpoints：

- `GET /api/v1/document-chunks`：列出全部 chunk。
- `GET /api/v1/document-chunks?knowledge_source_id=<knowledge-source-id>`：按知识来源过滤 chunk。
- `POST /api/v1/knowledge-sources/:knowledge_source_id/chunks`：对知识来源 summary 执行 deterministic splitter 并写入 chunk 事件。

Memory endpoints：

- `GET /api/v1/memories`：列出全部 memory metadata。
- `GET /api/v1/memories?knowledge_source_id=<knowledge-source-id>`：按知识来源过滤 memory metadata。
- `POST /api/v1/memories`：记录一条手动 memory metadata。

Search 与 trace endpoints：

- `POST /api/v1/search`：执行 deterministic keyword retrieval，返回 `query`、`hits`、`retrieval_trace_id`。
- `GET /api/v1/retrieval-traces`：列出检索轨迹，按 `created_at desc` 返回。

错误边界：无效 UUID 返回 bad request；未知 run、artifact、context bundle、knowledge source 返回 not found；空输入、空 query 或非法 confidence/limit 会被拒绝。

## Non-goals

Milestone 6 不提供：

- real embeddings 或 embedding job execution。
- Tantivy production index。
- vector database integration。
- worker runtime、调度器或后台 ingestion pipeline。
- OCR、PDF 或 web ingestion。
- automatic long-term memory。
- automatic agent recall。
- agent prompt injection 或自动把检索结果写入模型 prompt。
- memory ranking、semantic ranking 或 context ranking。
- 真实模型调用、真实工具执行、Tauri 打包、云同步或远端账号体系。

这些能力属于 M7 及之后的 worker / embedding execution、runtime orchestration 和更完整 ingestion 设计。

## Verification

本里程碑文档与实现的基础验证：

```bash
git diff --check
python3 - <<'PY'
from pathlib import Path
paths=[Path('README.md'), *Path('docs/architecture').glob('*.md')]
needles=['TB'+'D','TO'+'DO','place'+'holder','FIX'+'ME','?'+'?']
hits=[]
for path in paths:
    text=path.read_text()
    for needle in needles:
        if needle in text:
            hits.append((str(path), needle))
print('docs scan hits:', hits)
raise SystemExit(1 if hits else 0)
PY
. "$HOME/.cargo/env" && pnpm check
git status --short --branch
```

HTTP 手动验证可参考 README 的 Milestone 6 curl 示例。
