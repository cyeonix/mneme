# Milestone 5：Artifact Workspace 与 Context Inspector

Milestone 5 把 run lifecycle 和 tool activity 之后产生的运行产物纳入同一条可审计数据面：artifact 内容按 SHA-256 寻址，artifact/context 元数据写入 Operation Ledger 并投影到 SQLite 读模型，Workbench 只读展示 Artifact Workspace 与 Context Inspector。

## Scope

包含：

- 新增领域 ID：`ArtifactId`、`ContextBundleId`，都与 `RunId`、`EventId` 一样使用 UUID newtype，避免跨领域误传。
- 新增 artifact 领域模型：`ArtifactKind`、`ArtifactRecord`、`ArtifactDetail`、`StoredArtifactContent`。
- 新增 context bundle 领域模型：`ContextBundleRecord`，以标题、摘要、artifact ID 列表和 event ID 列表描述一次可复用上下文包。
- 新增 Operation Ledger 事件：`ArtifactCreated`、`ContextBundleCreated`。
- 新增 content-addressed artifact store：内容写入后计算 SHA-256，metadata 只保存 `sha256` 与 `byte_len`。
- 新增 SQLite `artifacts` 与 `context_bundles` 读模型。
- 新增 HTTP artifact/context endpoints 与 TypeScript client 方法。
- Workbench 新增只读 `Artifact Workspace` 与 `Context Inspector` 区块。

不包含：搜索、向量索引、记忆排序、真实 worker runtime、真实模型或工具执行、Tauri 打包、云同步、编辑/搜索/拖拽/投递 prompt compile UI。

## Event Flow

Artifact 创建流程：

```text
POST /api/v1/runs/:run_id/artifacts
  → ArtifactContentStore.put(content)
  → OperationEventKind::ArtifactCreated
  → operation_events append
  → artifacts projection upsert
  → running run updated_at refresh
```

Context bundle 创建流程：

```text
POST /api/v1/runs/:run_id/context-bundles
  → validate run_id, artifact_ids, event_ids
  → OperationEventKind::ContextBundleCreated
  → operation_events append
  → context_bundles projection upsert
  → running run updated_at refresh
```

`ArtifactCreated` 记录 `artifact_id`、`title`、`kind`、`mime_type`、`sha256`、`byte_len`。它不把完整内容放进 Operation Ledger。

`ContextBundleCreated` 记录 `context_bundle_id`、`title`、`summary`、`artifact_ids`、`event_ids`。它表达上下文包的组成，不复制 artifact 内容或 event payload。

## Content Addressed Store

Artifact 内容通过 `ArtifactContentStore` 写入：

- `put(content)` 计算 SHA-256 十六进制摘要。
- 相同内容复用相同 content address。
- `ArtifactRecord` 保存 `sha256` 与 `byte_len`，不保存完整正文。
- `get_artifact` 先读取 artifact metadata，再通过 `sha256` 取回内容；非 UTF-8 内容返回 `content: null`。

当前 Kernel 提供内存 content store 与本地 content-addressed store 实现。daemon 当前接入的是内存 content store，因此 M5 的默认本地 HTTP 验证关注领域边界、ledger/projection 和 UI 展示；跨进程重启后的 artifact 内容持久化不是本里程碑的默认运行承诺。

## Service Boundary

Operation Ledger 仍是事实源：

- `operation_events` 保存 append-only 事件流，是 run、tool、artifact、context 历史的审计边界。
- `runs`、`artifacts`、`context_bundles` 是可重建读模型。
- SQLite service 在一个事务内追加 `operation_events` 并更新对应投影。
- Context service 创建上下文包前会校验 run、artifact、event 引用已经存在。
- API route 与 Workbench 只能通过 Kernel service/client 访问数据，不直接写 SQLite。

这条边界让后续 worker runtime 或真实模型执行也能复用同一组 service，而不是绕过 Operation Ledger。

## HTTP API

Artifact endpoints：

- `GET /api/v1/artifacts`：列出 artifact metadata，按 `created_at desc` 返回。
- `POST /api/v1/runs/:run_id/artifacts`：创建文本类 artifact。
- `GET /api/v1/artifacts/:artifact_id`：返回 artifact metadata 与可解码文本内容。

创建 artifact request：

```json
{
  "title": "Run summary",
  "kind": "markdown",
  "mime_type": "text/markdown",
  "content": "# Summary\n\n完成了 Artifact Workspace。"
}
```

Context bundle endpoints：

- `GET /api/v1/context-bundles`：列出 context bundle metadata。
- `POST /api/v1/runs/:run_id/context-bundles`：创建上下文包。
- `GET /api/v1/context-bundles/:context_bundle_id`：读取一个上下文包。

创建 context bundle request：

```json
{
  "title": "Debug context",
  "summary": "User report and generated artifact",
  "artifact_ids": ["<artifact-id>"],
  "event_ids": []
}
```

错误边界：未知 run 返回 not found；无效 UUID 返回 bad request；空标题、空 mime type、空正文、未知 artifact/event 引用会被拒绝。

## Workbench

Workbench 在现有 Runs 与 Tool Activity 只读面板之后新增两个只读区块：

- `Artifact Workspace`：调用 `listArtifacts` 和 `getArtifact`，展示 artifact 标题、kind、mime type、SHA-256 前缀、byte length 与内容预览。
- `Context Inspector`：调用 `listContextBundles`，展示 context bundle 标题、摘要、artifact 数量、event 数量与创建时间。

M5 Workbench 只负责观察当前 Kernel 数据面。它不提供 artifact 编辑、搜索、拖拽、删除、重新排序或 prompt compile 操作。

## Non-goals

- 不做搜索、向量索引、memory ranking 或 context ranking。这些属于 Milestone 6。
- 不接真实 worker runtime、真实 LLM、真实 tool execution、streaming 或调度循环。这些属于 Milestone 7 及之后。
- 不做 Tauri 打包、系统权限弹窗、云同步或远端账号体系。
- 不做 artifact 编辑器、context 搜索框、drag/drop 编排、prompt compile UI 或自动上下文压缩。

## Verification

本里程碑文档与实现的基础验证：

```bash
git diff --check
python3 - <<'PY'
from pathlib import Path
paths=[Path('README.md'), *Path('docs/architecture').glob('*.md')]
needles=['TB'+'D','TO'+'DO','place'+'holder','FIX'+'ME','?'+'?']
hits=[]
for p in paths:
    text=p.read_text()
    for needle in needles:
        if needle in text:
            hits.append(f'{p}:{needle}')
print('docs scan hits:', hits)
raise SystemExit(1 if hits else 0)
PY
. "$HOME/.cargo/env" && pnpm check
git status --short
```

HTTP 手动验证可参考 README 的 artifact 与 context bundle curl 示例。
