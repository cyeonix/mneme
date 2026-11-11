import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  ArtifactDetail,
  ArtifactsResponse,
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
import { App } from './App';
import { ensureDaemonStarted, getDesktopRuntimeStatus, saveProviderConfig } from '../tauri/bridge';

vi.mock('../tauri/bridge', () => ({
  getDesktopRuntimeStatus: vi.fn(async () => ({
    daemon: { state: 'ready', api_base_url: 'http://127.0.0.1:3766' },
    provider: {
      base_url: 'https://api.openai.com/v1',
      model: 'gpt-4o-mini',
      timeout_secs: 30,
      has_api_key: false,
    },
  })),
  ensureDaemonStarted: vi.fn(async () => ({
    daemon: { state: 'ready', api_base_url: 'http://127.0.0.1:3766' },
    provider: {
      base_url: 'https://api.openai.com/v1',
      model: 'gpt-4o-mini',
      timeout_secs: 30,
      has_api_key: false,
    },
  })),
  saveProviderConfig: vi.fn(async (input: { base_url: string; model: string; timeout_secs: number; api_key: string }) => ({
    base_url: input.base_url,
    model: input.model,
    timeout_secs: input.timeout_secs,
    has_api_key: input.api_key.length > 0,
  })),
  clearProviderConfig: vi.fn(async () => ({
    base_url: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
    timeout_secs: 30,
    has_api_key: false,
  })),
}));

interface TestClientState {
  health?: KernelHealth;
  events?: EventsResponse;
  runs?: RunsResponse;
  artifacts?: ArtifactsResponse;
  artifactDetail?: ArtifactDetail;
  contextBundles?: ContextBundlesResponse;
  knowledgeSources?: KnowledgeSourcesResponse['knowledge_sources'];
  documentChunks?: DocumentChunksResponse['document_chunks'];
  memories?: MemoriesResponse['memories'];
  retrievalTraces?: RetrievalTracesResponse['retrieval_traces'];
  workerJobs?: WorkerJobsResponse;
}

function createTestClient(state: TestClientState = {}): MnemeApiClient {
  return {
    async getHealth() {
      return state.health ?? { service: 'mneme-kernel', status: 'ready', version: '0.1.0' };
    },
    async listEvents() {
      return state.events ?? { events: [] };
    },
    listRuns: vi.fn(async () => {
      return state.runs ?? {
        runs: [
          {
            run_id: 'run-1',
            status: 'running' as const,
            instruction: '展示 runs 投影',
            started_at: '2026-11-09T00:00:00Z',
            completed_at: null,
            failed_at: null,
            summary: null,
            error: null,
            updated_at: '2026-11-09T00:00:00Z',
          },
        ],
      };
    }),
    async createRun() {
      return { run_id: 'run-123' };
    },
    async requestModel() {
      return { run_id: 'run-123' };
    },
    async recordAssistantMessage() {
      return { run_id: 'run-123' };
    },
    async completeRun() {
      return { run_id: 'run-123' };
    },
    async failRun() {
      return { run_id: 'run-123' };
    },
    async requestToolCall() {
      return { run_id: 'run-123', tool_call_id: 'tool-123', approval: 'auto_approved' };
    },
    async approveToolCall() {
      return { run_id: 'run-123', tool_call_id: 'tool-123' };
    },
    async denyToolCall() {
      return { run_id: 'run-123', tool_call_id: 'tool-123' };
    },
    async recordToolResult() {
      return { run_id: 'run-123', tool_call_id: 'tool-123' };
    },
    async recordToolFailure() {
      return { run_id: 'run-123', tool_call_id: 'tool-123' };
    },
    async listArtifacts() {
      return state.artifacts ?? { artifacts: [] };
    },
    async createArtifact() {
      return { artifact_id: 'artifact-123' };
    },
    async getArtifact() {
      return (
        state.artifactDetail ?? {
          artifact: {
            artifact_id: 'artifact-123',
            run_id: 'run-123',
            title: 'Run summary',
            kind: 'markdown',
            mime_type: 'text/markdown',
            sha256: 'sha256',
            byte_len: 9,
            created_at: '2026-11-10T00:00:00Z',
          },
          content: '# Summary',
        }
      );
    },
    async listContextBundles() {
      return state.contextBundles ?? { context_bundles: [] };
    },
    async createContextBundle() {
      return { context_bundle_id: 'context-bundle-123' };
    },
    async getContextBundle() {
      return {
        context_bundle_id: 'context-bundle-123',
        run_id: 'run-123',
        title: 'Debug context',
        summary: 'User report',
        artifact_ids: [],
        event_ids: [],
        created_at: '2026-11-10T00:00:00Z',
      };
    },
    listKnowledgeSources: vi.fn(async () => ({ knowledge_sources: state.knowledgeSources ?? [] })),
    createKnowledgeSourceFromArtifact: vi.fn(async () => ({ knowledge_source_id: 'source-1' })),
    createKnowledgeSourceFromContextBundle: vi.fn(async () => ({ knowledge_source_id: 'source-1' })),
    createManualKnowledgeSource: vi.fn(async () => ({ knowledge_source_id: 'source-1' })),
    listDocumentChunks: vi.fn(async () => ({ document_chunks: state.documentChunks ?? [] })),
    chunkKnowledgeSource: vi.fn(async () => ({ document_chunks: [] })),
    listMemories: vi.fn(async () => ({ memories: state.memories ?? [] })),
    recordMemory: vi.fn(async () => ({ memory_id: 'memory-1' })),
    search: vi.fn(async () => ({ query: 'sqlite', hits: [], retrieval_trace_id: 'trace-1' })),
    listRetrievalTraces: vi.fn(async () => ({ retrieval_traces: state.retrievalTraces ?? [] })),
    runWorkerModelCompletion: vi.fn(async () => ({ run_id: 'run-123', worker_job_id: 'job-123' })),
    listWorkerJobs: vi.fn(async () => state.workerJobs ?? { worker_jobs: [] }),
  };
}

describe('App', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getDesktopRuntimeStatus).mockResolvedValue({
      daemon: { state: 'ready', api_base_url: 'http://127.0.0.1:3766' },
      provider: {
        base_url: 'https://api.openai.com/v1',
        model: 'gpt-4o-mini',
        timeout_secs: 30,
        has_api_key: false,
      },
    });
    vi.mocked(ensureDaemonStarted).mockResolvedValue({
      daemon: { state: 'ready', api_base_url: 'http://127.0.0.1:3766' },
      provider: {
        base_url: 'https://api.openai.com/v1',
        model: 'gpt-4o-mini',
        timeout_secs: 30,
        has_api_key: false,
      },
    });
    vi.mocked(saveProviderConfig).mockImplementation(async (input) => ({
      base_url: input.base_url,
      model: input.model,
      timeout_secs: input.timeout_secs,
      has_api_key: input.api_key.trim().length > 0,
    }));
  });

  it('shows desktop runtime and saves provider config without rendering the api key', async () => {
    render(<App client={createTestClient()} />);

    expect(await screen.findByText('Desktop Runtime')).toBeInTheDocument();
    expect(screen.getByText('Daemon: ready')).toBeInTheDocument();
    expect(screen.getByText('Provider key saved: no')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Provider base URL'), { target: { value: 'https://provider.example/v1' } });
    fireEvent.change(screen.getByLabelText('Provider model'), { target: { value: 'gpt-test' } });
    fireEvent.change(screen.getByLabelText('Provider API key'), { target: { value: 'secret-key' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save provider config' }));

    expect(await screen.findByText('Provider key saved: yes')).toBeInTheDocument();
    expect(screen.queryByText('secret-key')).not.toBeInTheDocument();
  });

  it('starts daemon only when user clicks the desktop action', async () => {
    render(<App client={createTestClient()} />);

    expect(ensureDaemonStarted).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole('button', { name: 'Start daemon' }));
    await waitFor(() => {
      expect(ensureDaemonStarted).toHaveBeenCalledTimes(1);
    });
  });
  it('渲染 Mneme Workbench 和 Kernel 健康状态', async () => {
    render(<App client={createTestClient()} />);

    expect(screen.getByRole('heading', { name: 'Mneme Workbench' })).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText('Kernel ready')).toBeInTheDocument();
    });
    expect(await screen.findByText('展示 runs 投影')).toBeInTheDocument();
    expect(screen.getByText('running')).toBeInTheDocument();
  });

  it('shows lifecycle result details for completed and failed runs', async () => {
    const client = createTestClient({
      runs: {
        runs: [
          {
            run_id: 'run-completed',
            status: 'completed' as const,
            instruction: '整理今天',
            started_at: '2026-11-10T00:00:00Z',
            completed_at: '2026-11-10T00:01:00Z',
            failed_at: null,
            summary: '总结完成',
            error: null,
            updated_at: '2026-11-10T00:01:00Z',
          },
          {
            run_id: 'run-failed',
            status: 'failed' as const,
            instruction: '失败示例',
            started_at: '2026-11-10T00:00:00Z',
            completed_at: null,
            failed_at: '2026-11-10T00:02:00Z',
            summary: null,
            error: '模型不可用',
            updated_at: '2026-11-10T00:02:00Z',
          },
        ],
      },
    });

    render(<App client={client} />);

    expect(await screen.findByText('总结完成')).toBeInTheDocument();
    expect(await screen.findByText('模型不可用')).toBeInTheDocument();
  });

  it('shows artifacts and context bundles after tool activity', async () => {
    const client = createTestClient({
      artifacts: {
        artifacts: [
          {
            artifact_id: 'artifact-123',
            run_id: 'run-123',
            title: 'Run summary',
            kind: 'markdown',
            mime_type: 'text/markdown',
            sha256: 'abcdef1234567890',
            byte_len: 9,
            created_at: '2026-11-10T00:00:00Z',
          },
        ],
      },
      artifactDetail: {
        artifact: {
          artifact_id: 'artifact-123',
          run_id: 'run-123',
          title: 'Run summary',
          kind: 'markdown',
          mime_type: 'text/markdown',
          sha256: 'abcdef1234567890',
          byte_len: 9,
          created_at: '2026-11-10T00:00:00Z',
        },
        content: '# Summary\n\nThis run completed successfully.',
      },
      contextBundles: {
        context_bundles: [
          {
            context_bundle_id: 'context-bundle-123',
            run_id: 'run-123',
            title: 'Debug context',
            summary: 'Useful context for debugging',
            artifact_ids: ['artifact-123'],
            event_ids: ['event-1', 'event-2'],
            created_at: '2026-11-10T00:00:00Z',
          },
        ],
      },
    });

    render(<App client={client} />);

    expect(await screen.findByText('Artifact Workspace')).toBeInTheDocument();
    expect(await screen.findByText('Run summary')).toBeInTheDocument();
    expect(await screen.findByText('markdown')).toBeInTheDocument();
    expect(await screen.findByText('abcdef12')).toBeInTheDocument();
    expect(await screen.findByText(/# Summary/)).toBeInTheDocument();
    expect(await screen.findByText('Context Inspector')).toBeInTheDocument();
    expect(await screen.findByText('Debug context')).toBeInTheDocument();
    expect(await screen.findByText('1 artifact')).toBeInTheDocument();
    expect(await screen.findByText('2 events')).toBeInTheDocument();
  });

  it('renders knowledge search memory and retrieval trace sections', async () => {
    const client = createTestClient({
      knowledgeSources: [
        {
          knowledge_source_id: 'source-1',
          run_id: 'run-123',
          source_kind: 'artifact',
          artifact_id: 'artifact-1',
          context_bundle_id: null,
          title: 'Run summary source',
          summary: 'SQLite deterministic retrieval source',
          created_at: '2026-11-10T00:00:00Z',
        },
      ],
      documentChunks: [
        {
          document_chunk_id: 'chunk-1',
          knowledge_source_id: 'source-1',
          run_id: 'run-123',
          sequence: 0,
          text: 'SQLite deterministic retrieval chunk',
          char_start: 0,
          char_end: 36,
          created_at: '2026-11-10T00:00:00Z',
        },
      ],
      memories: [
        {
          memory_id: 'memory-1',
          knowledge_source_id: 'source-1',
          run_id: 'run-123',
          kind: 'decision',
          title: 'Use SQLite-first search',
          body: 'Search sidecars are rebuildable.',
          confidence: 0.9,
          created_at: '2026-11-10T00:00:00Z',
        },
      ],
      retrievalTraces: [
        {
          retrieval_trace_id: 'trace-1',
          run_id: 'run-123',
          query: 'sqlite',
          result_chunk_ids: ['chunk-1'],
          result_memory_ids: ['memory-1'],
          explanation: 'M6 deterministic keyword retrieval matched sqlite',
          created_at: '2026-11-10T00:00:00Z',
        },
      ],
    });

    render(<App client={client} />);

    expect(await screen.findByText('Knowledge Search')).toBeInTheDocument();
    expect(screen.getByText('Run summary source')).toBeInTheDocument();
    expect(screen.getByText('SQLite deterministic retrieval chunk')).toBeInTheDocument();
    expect(screen.getByText('Memory Inspector')).toBeInTheDocument();
    expect(screen.getByText('Use SQLite-first search')).toBeInTheDocument();
    expect(screen.getByText('Retrieval Traces')).toBeInTheDocument();
    expect(screen.getByText('M6 deterministic keyword retrieval matched sqlite')).toBeInTheDocument();
    expect(
      screen.getByText('Milestone 6 search uses deterministic keyword retrieval, not semantic embeddings.'),
    ).toBeInTheDocument();
    expect(client.search).not.toHaveBeenCalled();
  });

  it('runs provider completion only after explicit user action', async () => {
    const client = createTestClient({
      runs: {
        runs: [
          {
            run_id: 'run-123',
            status: 'running',
            instruction: 'Summarizable run',
            started_at: '2026-11-09T00:00:00Z',
            completed_at: null,
            failed_at: null,
            summary: null,
            error: null,
            updated_at: '2026-11-09T00:00:00Z',
          },
        ],
      },
    });

    render(<App client={client} />);

    expect(client.runWorkerModelCompletion).not.toHaveBeenCalled();

    fireEvent.change(await screen.findByLabelText('Worker run'), { target: { value: 'run-123' } });
    fireEvent.change(screen.getByLabelText('Worker prompt'), { target: { value: 'Summarize this run' } });
    fireEvent.click(screen.getByRole('button', { name: 'Run provider completion' }));

    await waitFor(() => {
      expect(client.runWorkerModelCompletion).toHaveBeenCalledWith('run-123', {
        prompt: 'Summarize this run',
      });
    });
    await waitFor(() => {
      expect(client.listWorkerJobs).toHaveBeenCalledTimes(2);
      expect(client.listRuns).toHaveBeenCalledTimes(2);
    });
  });

  it('does not run provider completion for blank prompt or missing run selection', async () => {
    const client = createTestClient({
      runs: {
        runs: [
          {
            run_id: 'run-123',
            status: 'running',
            instruction: 'Summarizable run',
            started_at: '2026-11-09T00:00:00Z',
            completed_at: null,
            failed_at: null,
            summary: null,
            error: null,
            updated_at: '2026-11-09T00:00:00Z',
          },
        ],
      },
    });

    render(<App client={client} />);

    fireEvent.change(await screen.findByLabelText('Worker prompt'), { target: { value: 'Summarize this run' } });
    fireEvent.click(screen.getByRole('button', { name: 'Run provider completion' }));
    expect(client.runWorkerModelCompletion).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText('Worker run'), { target: { value: 'run-123' } });
    fireEvent.change(screen.getByLabelText('Worker prompt'), { target: { value: '   ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Run provider completion' }));
    expect(client.runWorkerModelCompletion).not.toHaveBeenCalled();
  });

  it('describes worker completion as non-streaming and not tool execution', async () => {
    render(<App client={createTestClient()} />);

    expect(
      await screen.findByText(
        'Runs a non-streaming provider completion through the local daemon. This does not execute tools.',
      ),
    ).toBeInTheDocument();
  });

  it('shows worker runtime jobs without claiming streaming or tool execution', async () => {
    const client = createTestClient({
      workerJobs: {
        worker_jobs: [
          {
            worker_job_id: 'job-1',
            worker_id: 'worker-1',
            run_id: 'run-1',
            kind: 'model_completion',
            status: 'completed',
            model: 'gpt-test',
            error: null,
            started_at: '2026-01-01T00:00:00Z',
            completed_at: '2026-01-01T00:00:01Z',
          },
          {
            worker_job_id: 'job-2',
            worker_id: 'worker-1',
            run_id: 'run-2',
            kind: 'model_completion',
            status: 'failed',
            model: 'gpt-test',
            error: 'provider returned 500',
            started_at: '2026-01-01T00:00:02Z',
            completed_at: '2026-01-01T00:00:03Z',
          },
        ],
      },
    });

    render(<App client={client} />);

    expect(await screen.findByText('Worker Runtime')).toBeInTheDocument();
    expect(screen.getAllByText('model_completion')).toHaveLength(2);
    expect(screen.getByText('completed')).toBeInTheDocument();
    expect(screen.getByText('failed')).toBeInTheDocument();
    expect(screen.getByText('provider returned 500')).toBeInTheDocument();
    expect(
      screen.getByText(
        'Provider keys are configured in Desktop Runtime. No streaming token UI or tool execution is available in Milestone 8.',
      ),
    ).toBeInTheDocument();
    expect(client.runWorkerModelCompletion).not.toHaveBeenCalled();
  });

  it('shows tool activity from the operation ledger', async () => {
    const client = createTestClient({
      events: {
        events: [
          {
            id: 'event-1',
            run_id: 'run-123',
            occurred_at: '2026-11-10T00:00:00Z',
            kind: {
              type: 'tool_call_requested',
              tool_call_id: 'tool-123',
              tool_name: 'read_file',
              effect: 'read_only',
              input_json: { path: 'README.md' },
            },
          },
          {
            id: 'event-2',
            run_id: 'run-123',
            occurred_at: '2026-11-10T00:00:01Z',
            kind: {
              type: 'tool_call_approved',
              tool_call_id: 'tool-123',
              approved_by: 'mneme-policy',
            },
          },
          {
            id: 'event-3',
            run_id: 'run-123',
            occurred_at: '2026-11-10T00:00:02Z',
            kind: {
              type: 'tool_call_completed',
              tool_call_id: 'tool-123',
              output_json: { content: 'hello' },
            },
          },
          {
            id: 'event-4',
            run_id: 'run-123',
            occurred_at: '2026-11-10T00:00:03Z',
            kind: {
              type: 'tool_call_failed',
              tool_call_id: 'tool-456',
              reason: '工具执行失败',
            },
          },
          {
            id: 'event-5',
            run_id: 'run-123',
            occurred_at: '2026-11-10T00:00:04Z',
            kind: {
              type: 'tool_call_denied',
              tool_call_id: 'tool-789',
              reason: '用户拒绝执行',
            },
          },
        ],
      },
    });

    render(<App client={client} />);

    expect(await screen.findByText('Tool Activity')).toBeInTheDocument();
    expect(await screen.findByText('read_file')).toBeInTheDocument();
    expect(await screen.findByText('read_only')).toBeInTheDocument();
    expect(await screen.findByText('mneme-policy')).toBeInTheDocument();
    expect(await screen.findByText('completed')).toBeInTheDocument();
    expect(await screen.findByText('result recorded')).toBeInTheDocument();
    expect(await screen.findByText('tool_call_failed')).toBeInTheDocument();
    expect(await screen.findByText('工具执行失败')).toBeInTheDocument();
    expect(await screen.findByText('tool_call_denied')).toBeInTheDocument();
    expect(await screen.findByText('用户拒绝执行')).toBeInTheDocument();
  });
});
