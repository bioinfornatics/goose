/**
 * Integration tests for the Pipeline CRUD API.
 *
 * These tests spawn a real goosed process and issue requests via the
 * auto-generated API client to verify pipeline operations work correctly.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { setupGoosed, type GoosedTestContext } from './setup';

describe('Pipeline CRUD API', () => {
  let ctx: GoosedTestContext;

  beforeAll(async () => {
    ctx = await setupGoosed({});
  }, 30_000);

  afterAll(async () => {
    await ctx?.cleanup();
  });

  it('should list pipelines (initially empty)', async () => {
    const response = await fetch(`${ctx.baseUrl}/pipelines`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    expect(response.status).toBe(200);
    const pipelines = await response.json();
    expect(Array.isArray(pipelines)).toBe(true);
    expect(pipelines.length).toBe(0);
  });

  it('should create a pipeline', async () => {
    const pipeline = {
      name: 'Test Pipeline',
      description: 'A simple test pipeline',
      nodes: [
        { id: 'trigger-1', kind: 'trigger', label: 'Start' },
        { id: 'agent-1', kind: 'agent', label: 'Process' },
      ],
      edges: [{ source: 'trigger-1', target: 'agent-1' }],
    };

    const response = await fetch(`${ctx.baseUrl}/pipelines`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': ctx.secretKey,
      },
      body: JSON.stringify(pipeline),
    });
    expect(response.status).toBe(201);
    const created = await response.json();
    expect(created.name).toBe('Test Pipeline');
    expect(created.nodes).toHaveLength(2);
    expect(created.edges).toHaveLength(1);
  });

  it('should list pipelines after creation', async () => {
    const response = await fetch(`${ctx.baseUrl}/pipelines`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    expect(response.status).toBe(200);
    const pipelines = await response.json();
    expect(pipelines.length).toBeGreaterThanOrEqual(1);
    expect(pipelines[0].name).toBe('Test Pipeline');
  });

  it('should get a pipeline by id', async () => {
    // First get the list to find the id
    const listResponse = await fetch(`${ctx.baseUrl}/pipelines`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    const pipelines = await listResponse.json();
    const id = pipelines[0].id;

    const response = await fetch(`${ctx.baseUrl}/pipelines/${id}`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    expect(response.status).toBe(200);
    const pipeline = await response.json();
    expect(pipeline.name).toBe('Test Pipeline');
    expect(pipeline.nodes).toHaveLength(2);
  });

  it('should update a pipeline', async () => {
    const listResponse = await fetch(`${ctx.baseUrl}/pipelines`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    const pipelines = await listResponse.json();
    const id = pipelines[0].id;

    const updated = {
      name: 'Updated Pipeline',
      description: 'Updated description',
      nodes: [
        { id: 'trigger-1', kind: 'trigger', label: 'Start' },
        { id: 'agent-1', kind: 'agent', label: 'Step 1' },
        { id: 'agent-2', kind: 'agent', label: 'Step 2' },
      ],
      edges: [
        { source: 'trigger-1', target: 'agent-1' },
        { source: 'agent-1', target: 'agent-2' },
      ],
    };

    const response = await fetch(`${ctx.baseUrl}/pipelines/${id}`, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': ctx.secretKey,
      },
      body: JSON.stringify(updated),
    });
    expect(response.status).toBe(200);
    const result = await response.json();
    expect(result.name).toBe('Updated Pipeline');
    expect(result.nodes).toHaveLength(3);
  });

  it('should validate a pipeline with cycle and reject', async () => {
    const badPipeline = {
      name: 'Cyclic Pipeline',
      nodes: [
        { id: 'a', kind: 'agent', label: 'A' },
        { id: 'b', kind: 'agent', label: 'B' },
      ],
      edges: [
        { source: 'a', target: 'b' },
        { source: 'b', target: 'a' },
      ],
    };

    const response = await fetch(`${ctx.baseUrl}/pipelines/validate`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': ctx.secretKey,
      },
      body: JSON.stringify(badPipeline),
    });
    expect(response.status).toBe(200);
    const result = await response.json();
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
    expect(result.errors.some((e: string) => e.toLowerCase().includes('cycle'))).toBe(true);
  });

  it('should validate a valid pipeline', async () => {
    const goodPipeline = {
      name: 'Good Pipeline',
      nodes: [
        { id: 'trigger-1', kind: 'trigger', label: 'Start' },
        { id: 'agent-1', kind: 'agent', label: 'Work' },
      ],
      edges: [{ source: 'trigger-1', target: 'agent-1' }],
    };

    const response = await fetch(`${ctx.baseUrl}/pipelines/validate`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': ctx.secretKey,
      },
      body: JSON.stringify(goodPipeline),
    });
    expect(response.status).toBe(200);
    const result = await response.json();
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it('should delete a pipeline', async () => {
    const listResponse = await fetch(`${ctx.baseUrl}/pipelines`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    const pipelines = await listResponse.json();
    const id = pipelines[0].id;

    const response = await fetch(`${ctx.baseUrl}/pipelines/${id}`, {
      method: 'DELETE',
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    expect(response.status).toBe(204);

    // Verify it's gone
    const getResponse = await fetch(`${ctx.baseUrl}/pipelines/${id}`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    expect(getResponse.status).toBe(404);
  });

  it('should return 404 for non-existent pipeline', async () => {
    const response = await fetch(`${ctx.baseUrl}/pipelines/does-not-exist`, {
      headers: { 'X-Secret-Key': ctx.secretKey },
    });
    expect(response.status).toBe(404);
  });
});
