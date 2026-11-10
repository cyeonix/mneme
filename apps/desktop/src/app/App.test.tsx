import '@testing-library/jest-dom/vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type {
  ArtifactDetail,
  ArtifactsResponse,
  ContextBundlesResponse,
  EventsResponse,
  KernelHealth,
  MnemeApiClient,
  RunsResponse,
} from '../api/client';
import { App } from './App';

interface TestClientState {
  health?: KernelHealth;
  events?: EventsResponse;
  runs?: RunsResponse;
  artifacts?: ArtifactsResponse;
  artifactDetail?: ArtifactDetail;
  contextBundles?: ContextBundlesResponse;
}

function createTestClient(state: TestClientState = {}): MnemeApiClient {
  return {
    async getHealth() {
      return state.health ?? { service: 'mneme-kernel', status: 'ready', version: '0.1.0' };
    },
    async listEvents() {
      return state.events ?? { events: [] };
    },
    async listRuns() {
      return state.runs ?? {
        runs: [
          {
            run_id: 'run-1',
            status: 'running',
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
    },
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
  };
}

describe('App', () => {
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
