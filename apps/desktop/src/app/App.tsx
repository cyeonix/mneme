import { useEffect, useState } from 'react';
import type {
  ArtifactDetail,
  ArtifactRecord,
  ContextBundlesResponse,
  DocumentChunksResponse,
  EventsResponse,
  KernelHealth,
  KnowledgeSourcesResponse,
  MemoriesResponse,
  MnemeApiClient,
  RetrievalTracesResponse,
  RunsResponse,
  WorkerJobsResponse,
} from '../api/client';

interface AppProps {
  client: MnemeApiClient;
}

interface ToolActivityItem {
  eventId: string;
  runId: string;
  occurredAt: string;
  toolCallId: string;
  label: string;
  detail: string;
  effect?: string;
}

interface ArtifactPreviewItem {
  record: ArtifactRecord;
  preview: string;
}

const TEXT_ARTIFACT_KINDS = new Set(['text', 'markdown', 'json']);
const BINARY_PREVIEW_MESSAGE = 'Binary artifact preview is not available in Milestone 5.';

function artifactPreview(detail: ArtifactDetail): string {
  if (!TEXT_ARTIFACT_KINDS.has(detail.artifact.kind)) {
    return BINARY_PREVIEW_MESSAGE;
  }
  return (detail.content ?? '').slice(0, 240);
}

function countLabel(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

function toolActivityFromEvents(events: EventsResponse): ToolActivityItem[] {
  return events.events.flatMap((event) => {
    const kind = event.kind as Record<string, unknown>;
    if (kind.type === 'tool_call_requested') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: String(kind.tool_name),
          detail: 'requested',
          effect: String(kind.effect),
        },
      ];
    }
    if (kind.type === 'tool_call_approved') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: 'approved',
          detail: String(kind.approved_by),
        },
      ];
    }
    if (kind.type === 'tool_call_denied' || kind.type === 'tool_call_failed') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: String(kind.type),
          detail: String(kind.reason),
        },
      ];
    }
    if (kind.type === 'tool_call_completed') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: 'completed',
          detail: 'result recorded',
        },
      ];
    }
    return [];
  });
}

export function App({ client }: AppProps) {
  const [health, setHealth] = useState<KernelHealth | null>(null);
  const [events, setEvents] = useState<EventsResponse>({ events: [] });
  const [runs, setRuns] = useState<RunsResponse>({ runs: [] });
  const [artifactPreviews, setArtifactPreviews] = useState<ArtifactPreviewItem[]>([]);
  const [contextBundles, setContextBundles] = useState<ContextBundlesResponse>({ context_bundles: [] });
  const [knowledgeSources, setKnowledgeSources] = useState<KnowledgeSourcesResponse>({ knowledge_sources: [] });
  const [documentChunks, setDocumentChunks] = useState<DocumentChunksResponse>({ document_chunks: [] });
  const [memories, setMemories] = useState<MemoriesResponse>({ memories: [] });
  const [retrievalTraces, setRetrievalTraces] = useState<RetrievalTracesResponse>({ retrieval_traces: [] });
  const [workerJobs, setWorkerJobs] = useState<WorkerJobsResponse>({ worker_jobs: [] });
  const [error, setError] = useState<string | null>(null);
  const toolActivity = toolActivityFromEvents(events);

  useEffect(() => {
    let cancelled = false;

    async function loadKernelState() {
      try {
        // Workbench 通过 API 边界读取 Kernel 状态、Operation Ledger 事件和 runs 读模型。
        const [
          nextHealth,
          nextEvents,
          nextRuns,
          nextArtifacts,
          nextContextBundles,
          nextKnowledgeSources,
          nextDocumentChunks,
          nextMemories,
          nextRetrievalTraces,
          nextWorkerJobs,
        ] = await Promise.all([
          client.getHealth(),
          client.listEvents(),
          client.listRuns(),
          client.listArtifacts(),
          client.listContextBundles(),
          client.listKnowledgeSources(),
          client.listDocumentChunks(),
          client.listMemories(),
          client.listRetrievalTraces(),
          client.listWorkerJobs(),
        ]);
        const nextArtifactPreviews = await Promise.all(
          nextArtifacts.artifacts.map(async (record) => {
            if (record.kind === 'binary') {
              return { record, preview: BINARY_PREVIEW_MESSAGE };
            }
            const detail = await client.getArtifact(record.artifact_id);
            return { record, preview: artifactPreview(detail) };
          }),
        );
        if (!cancelled) {
          setHealth(nextHealth);
          setEvents(nextEvents);
          setRuns(nextRuns);
          setArtifactPreviews(nextArtifactPreviews);
          setContextBundles(nextContextBundles);
          setKnowledgeSources(nextKnowledgeSources);
          setDocumentChunks(nextDocumentChunks);
          setMemories(nextMemories);
          setRetrievalTraces(nextRetrievalTraces);
          setWorkerJobs(nextWorkerJobs);
        }
      } catch (unknownError) {
        if (!cancelled) {
          setError(unknownError instanceof Error ? unknownError.message : 'Unknown Mneme API error');
        }
      }
    }

    void loadKernelState();

    return () => {
      cancelled = true;
    };
  }, [client]);

  return (
    <main className="mneme-shell">
      <section className="hero-panel">
        <p className="eyebrow">Personal AI Kernel</p>
        <h1>Mneme Workbench</h1>
        <p className="lede">A local-first control room for runs, context, tools, artifacts, and memory.</p>
      </section>

      <section className="status-grid" aria-label="Kernel status">
        <article className="status-card">
          <span className="label">Kernel</span>
          <strong>{health ? `Kernel ${health.status}` : 'Connecting to kernel'}</strong>
          <span className="meta">{health ? `${health.service} ${health.version}` : 'Waiting for local daemon'}</span>
        </article>
        <article className="status-card">
          <span className="label">Operation Ledger</span>
          <strong>{events.events.length} events</strong>
          <span className="meta">Append-only event stream foundation</span>
        </article>
      </section>

      <section className="panel">
        <h2>Runs</h2>
        {runs.runs.length === 0 ? (
          <p className="muted">还没有持久化 run。通过 API 创建 run 后会显示在这里。</p>
        ) : (
          <ul className="run-list">
            {runs.runs.map((run) => (
              <li key={run.run_id} className="run-card">
                <span className="run-status">{run.status}</span>
                <strong>{run.instruction}</strong>
                <small>{run.updated_at}</small>
                {run.summary ? (
                  <p className="run-result">
                    总结：<span>{run.summary}</span>
                  </p>
                ) : null}
                {run.error ? (
                  <p className="run-error">
                    错误：<span>{run.error}</span>
                  </p>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>Tool Activity</h2>
        {toolActivity.length === 0 ? (
          <p className="muted">还没有工具请求。Milestone 4 只展示工具意图和审批记录，不执行真实工具。</p>
        ) : (
          <ul className="tool-activity-list">
            {toolActivity.map((activity) => (
              <li key={activity.eventId} className="tool-activity-card">
                <span className="tool-activity-kind">{activity.label}</span>
                {activity.effect ? <span className="tool-effect">{activity.effect}</span> : null}
                <strong>{activity.detail}</strong>
                <small>{activity.occurredAt}</small>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>Artifact Workspace</h2>
        {artifactPreviews.length === 0 ? (
          <p className="muted">还没有 artifact。Artifact Workspace 会只读展示已保存的运行产物。</p>
        ) : (
          <ul className="artifact-list">
            {artifactPreviews.map(({ record, preview }) => (
              <li key={record.artifact_id} className="artifact-card">
                <div className="artifact-card-header">
                  <strong>{record.title}</strong>
                  <span className="artifact-kind">{record.kind}</span>
                </div>
                <div className="artifact-meta">
                  <span>{record.mime_type}</span>
                  <span>{record.sha256.slice(0, 8)}</span>
                  <span>{record.byte_len} bytes</span>
                </div>
                <pre className="artifact-preview">{preview}</pre>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>Context Inspector</h2>
        {contextBundles.context_bundles.length === 0 ? (
          <p className="muted">还没有 context bundle。Context Inspector 会只读展示上下文包组成。</p>
        ) : (
          <ul className="context-list">
            {contextBundles.context_bundles.map((bundle) => (
              <li key={bundle.context_bundle_id} className="context-card">
                <strong>{bundle.title}</strong>
                <p>{bundle.summary}</p>
                <div className="context-counts">
                  <span>{countLabel(bundle.artifact_ids.length, 'artifact', 'artifacts')}</span>
                  <span>{countLabel(bundle.event_ids.length, 'event', 'events')}</span>
                </div>
                <small>{bundle.created_at}</small>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>Knowledge Search</h2>
        <p className="muted">Milestone 6 search uses deterministic keyword retrieval, not semantic embeddings.</p>
        <p className="muted">Read-only inspector: existing sources and chunks are displayed without running a search.</p>
        <div className="knowledge-grid">
          <article className="knowledge-card">
            <h3>Sources</h3>
            {knowledgeSources.knowledge_sources.length === 0 ? (
              <p className="muted">No knowledge sources have been recorded yet.</p>
            ) : (
              <ul className="knowledge-list">
                {knowledgeSources.knowledge_sources.map((source) => (
                  <li key={source.knowledge_source_id} className="knowledge-card">
                    <div className="artifact-card-header">
                      <strong>{source.title}</strong>
                      <span className="artifact-kind">{source.source_kind}</span>
                    </div>
                    <p>{source.summary}</p>
                    <small>{source.created_at}</small>
                  </li>
                ))}
              </ul>
            )}
          </article>
          <article className="knowledge-card">
            <h3>Chunks</h3>
            {documentChunks.document_chunks.length === 0 ? (
              <p className="muted">No document chunks have been generated yet.</p>
            ) : (
              <ul className="knowledge-list">
                {documentChunks.document_chunks.map((chunk) => (
                  <li key={chunk.document_chunk_id} className="knowledge-card">
                    <span className="label">Chunk {chunk.sequence}</span>
                    <p>{chunk.text}</p>
                    <small>
                      chars {chunk.char_start}-{chunk.char_end}
                    </small>
                  </li>
                ))}
              </ul>
            )}
          </article>
        </div>
      </section>

      <section className="panel">
        <h2>Memory Inspector</h2>
        {memories.memories.length === 0 ? (
          <p className="muted">No memories have been recorded yet. This panel is read-only.</p>
        ) : (
          <ul className="knowledge-list">
            {memories.memories.map((memory) => (
              <li key={memory.memory_id} className="memory-card">
                <div className="artifact-card-header">
                  <strong>{memory.title}</strong>
                  <span className="artifact-kind">{memory.kind}</span>
                </div>
                <p>{memory.body}</p>
                <small>
                  confidence {memory.confidence} · {memory.created_at}
                </small>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>Retrieval Traces</h2>
        {retrievalTraces.retrieval_traces.length === 0 ? (
          <p className="muted">No retrieval traces have been recorded yet. Searches are not run automatically on load.</p>
        ) : (
          <ul className="knowledge-list">
            {retrievalTraces.retrieval_traces.map((trace) => (
              <li key={trace.retrieval_trace_id} className="trace-card">
                <strong>{trace.query}</strong>
                <div className="context-counts">
                  <span>{countLabel(trace.result_chunk_ids.length, 'chunk', 'chunks')}</span>
                  <span>{countLabel(trace.result_memory_ids.length, 'memory', 'memories')}</span>
                </div>
                <p>{trace.explanation}</p>
                <small>{trace.created_at}</small>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>Worker Runtime</h2>
        <p className="muted">
          Provider keys are configured outside the Workbench. No streaming token UI or tool execution is available in
          Milestone 7.
        </p>
        {workerJobs.worker_jobs.length === 0 ? (
          <p className="muted">No worker jobs have been recorded yet.</p>
        ) : (
          <ul className="worker-job-list">
            {workerJobs.worker_jobs.map((job) => (
              <li key={job.worker_job_id} className="worker-job-card">
                <div className="artifact-card-header">
                  <strong>{job.kind}</strong>
                  <span className="artifact-kind">{job.status}</span>
                </div>
                <p>run {job.run_id}</p>
                {job.model ? <p>model {job.model}</p> : null}
                {job.error ? <p className="run-error">{job.error}</p> : null}
                <small>
                  {job.started_at}
                  {job.completed_at ? ` → ${job.completed_at}` : ''}
                </small>
              </li>
            ))}
          </ul>
        )}
      </section>

      {error ? <p className="error-banner">{error}</p> : null}
    </main>
  );
}
