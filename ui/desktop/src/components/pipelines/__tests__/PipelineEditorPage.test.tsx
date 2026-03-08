import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { PipelineEditorPage } from '../PipelineEditorPage';
import { getPipeline, updatePipeline } from '../../../api';
import type { Pipeline } from '../../../api/types.gen';

vi.mock('../../../api', () => ({
  getPipeline: vi.fn(),
  updatePipeline: vi.fn(),
}));

const mockGetPipeline = getPipeline as ReturnType<typeof vi.fn>;
void updatePipeline;

vi.mock('@xyflow/react', () => ({
  ReactFlowProvider: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ReactFlow: () => <div data-testid="reactflow-canvas">ReactFlow Canvas</div>,
  useNodesState: () => [[], vi.fn(), vi.fn()],
  useEdgesState: () => [[], vi.fn(), vi.fn()],
  Background: () => null,
  Controls: () => null,
  MiniMap: () => null,
  Panel: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  useReactFlow: () => ({ fitView: vi.fn() }),
  Position: { Top: 'top', Bottom: 'bottom', Left: 'left', Right: 'right' },
  Handle: () => null,
  MarkerType: { ArrowClosed: 'arrowclosed' },
}));

function renderEditor(id = 'test-id') {
  return render(
    <MemoryRouter initialEntries={[`/pipelines/${id}`]}>
      <Routes>
        <Route path="/pipelines/:id" element={<PipelineEditorPage />} />
        <Route path="/pipelines" element={<div>Pipeline List</div>} />
      </Routes>
    </MemoryRouter>,
  );
}

const samplePipeline: Pipeline = {
  apiVersion: 'goose/v1',
  kind: 'Pipeline',
  name: 'Test Pipeline',
  description: 'A test',
  nodes: [
    { id: 'n1', kind: 'trigger', label: 'Start', config: {} },
  ],
  edges: [],
};

function mockGetSuccess(pipeline: Pipeline) {
  mockGetPipeline.mockResolvedValue({
    data: { pipeline },
    error: undefined,
    request: new globalThis.Request('http://localhost/test'),
    response: new globalThis.Response(),
  });
}

describe('PipelineEditorPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows loading state initially', () => {
    mockGetPipeline.mockReturnValue(new Promise(() => {}));

    renderEditor();

    expect(screen.getByText('Loading pipeline...')).toBeInTheDocument();
  });

  it('renders pipeline name after load', async () => {
    mockGetSuccess(samplePipeline);

    renderEditor();

    await waitFor(() => {
      expect(screen.getByText('Test Pipeline')).toBeInTheDocument();
    });
  });

  it('renders Save button', async () => {
    mockGetSuccess(samplePipeline);

    renderEditor();

    await waitFor(() => {
      expect(screen.getByText('Save')).toBeInTheDocument();
    });
  });

  it('renders node palette', async () => {
    mockGetSuccess(samplePipeline);

    renderEditor();

    await waitFor(() => {
      expect(screen.getByText('Nodes')).toBeInTheDocument();
    });
  });

  it('shows error state on API failure', async () => {
    mockGetPipeline.mockRejectedValue(new Error('Network error'));

    renderEditor();

    await waitFor(() => {
      expect(screen.getByText('Failed to load pipeline')).toBeInTheDocument();
    });
  });

  it('shows Back to Pipelines button on error', async () => {
    mockGetPipeline.mockRejectedValue(new Error('Network error'));

    renderEditor();

    await waitFor(() => {
      expect(screen.getByText('Back to Pipelines')).toBeInTheDocument();
    });
  });

  it('calls getPipeline with correct id', async () => {
    mockGetSuccess(samplePipeline);

    renderEditor('my-pipeline-id');

    await waitFor(() => {
      expect(mockGetPipeline).toHaveBeenCalledWith(
        expect.objectContaining({ path: { id: 'my-pipeline-id' } }),
      );
    });
  });
});
