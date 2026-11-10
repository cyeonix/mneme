import '@testing-library/jest-dom/vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { MnemeApiClient } from '../api/client';
import { App } from './App';

function createClient(): MnemeApiClient {
  return {
    async getHealth() {
      return { service: 'mneme-kernel', status: 'ready', version: '0.1.0' };
    },
    async listEvents() {
      return { events: [] };
    },
    async listRuns() {
      return {
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
  };
}

describe('App', () => {
  it('渲染 Mneme Workbench 和 Kernel 健康状态', async () => {
    render(<App client={createClient()} />);

    expect(screen.getByRole('heading', { name: 'Mneme Workbench' })).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText('Kernel ready')).toBeInTheDocument();
    });
    expect(await screen.findByText('展示 runs 投影')).toBeInTheDocument();
    expect(screen.getByText('running')).toBeInTheDocument();
  });

  it('shows lifecycle result details for completed and failed runs', async () => {
    const client: MnemeApiClient = {
      getHealth: vi.fn(async () => ({ service: 'mneme-kernel', status: 'ready', version: '0.1.0' })),
      listEvents: vi.fn(async () => ({ events: [] })),
      listRuns: vi.fn(async () => ({
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
      })),
      createRun: vi.fn(),
      requestModel: vi.fn(),
      recordAssistantMessage: vi.fn(),
      completeRun: vi.fn(),
      failRun: vi.fn(),
      requestToolCall: vi.fn(async () => ({
        run_id: 'run-123',
        tool_call_id: 'tool-123',
        approval: 'auto_approved' as const,
      })),
      approveToolCall: vi.fn(async () => ({ run_id: 'run-123', tool_call_id: 'tool-123' })),
      denyToolCall: vi.fn(async () => ({ run_id: 'run-123', tool_call_id: 'tool-123' })),
      recordToolResult: vi.fn(async () => ({ run_id: 'run-123', tool_call_id: 'tool-123' })),
      recordToolFailure: vi.fn(async () => ({ run_id: 'run-123', tool_call_id: 'tool-123' })),
    };

    render(<App client={client} />);

    expect(await screen.findByText('总结完成')).toBeInTheDocument();
    expect(await screen.findByText('模型不可用')).toBeInTheDocument();
  });
});
