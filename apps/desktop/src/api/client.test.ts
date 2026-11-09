import { describe, expect, it, vi } from 'vitest';
import { createMnemeApiClient } from './client';

describe('createMnemeApiClient', () => {
  it('从配置的 base URL 读取 Kernel 健康状态', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ service: 'mneme-kernel', status: 'ready', version: '0.1.0' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const client = createMnemeApiClient('http://127.0.0.1:3766', fetchMock);

    const health = await client.getHealth();

    expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/health', {
      headers: { accept: 'application/json' },
    });
    expect(health.status).toBe('ready');
  });

  it('提交 run instruction 并返回新 run id', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ run_id: 'run-123' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const client = createMnemeApiClient('http://127.0.0.1:3766', fetchMock);

    const response = await client.createRun('Draft the first Mneme task');

    expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/runs', {
      method: 'POST',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify({ instruction: 'Draft the first Mneme task' }),
    });
    expect(response.run_id).toBe('run-123');
  });

  it('lists run projections', async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(
        JSON.stringify({
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
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const client = createMnemeApiClient('http://127.0.0.1:3766/', fetchImpl);

    await expect(client.listRuns()).resolves.toEqual({
      runs: [expect.objectContaining({ instruction: '展示 runs 投影', status: 'running' })],
    });
    expect(fetchImpl).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/runs', expect.any(Object));
  });
});
