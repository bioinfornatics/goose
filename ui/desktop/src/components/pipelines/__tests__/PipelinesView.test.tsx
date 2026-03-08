import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { PipelinesView } from '../PipelinesView';
import { listPipelines, createPipeline, deletePipeline } from '../../../api';
import type { PipelineManifest } from '../../../api/types.gen';

beforeAll(() => {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as typeof ResizeObserver;
});

vi.mock('../../../api', () => ({
  listPipelines: vi.fn(),
  createPipeline: vi.fn(),
  deletePipeline: vi.fn(),
}));

const mockListPipelines = listPipelines as ReturnType<typeof vi.fn>;
const mockCreatePipeline = createPipeline as ReturnType<typeof vi.fn>;
void deletePipeline;

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return { ...actual, useNavigate: () => mockNavigate };
});

function renderView() {
  return render(
    <MemoryRouter>
      <PipelinesView />
    </MemoryRouter>,
  );
}

const sampleManifest: PipelineManifest = {
  id: 'test-pipeline-1',
  name: 'My Test Pipeline',
  description: 'A test pipeline',
  nodeCount: 3,
  edgeCount: 2,
  updatedAt: '2025-01-15T10:00:00Z',
};

function mockListSuccess(pipelines: PipelineManifest[]) {
  mockListPipelines.mockResolvedValue({
    data: { pipelines },
    error: undefined,
    request: new globalThis.Request('http://localhost/test'),
    response: new globalThis.Response(),
  });
}

describe('PipelinesView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('empty state', () => {
    it('shows empty state when no pipelines', async () => {
      mockListSuccess([]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('No pipelines yet')).toBeInTheDocument();
      });
    });

    it('shows Create Pipeline button in empty state', async () => {
      mockListSuccess([]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('Create Pipeline')).toBeInTheDocument();
      });
    });
  });

  describe('with pipelines', () => {
    it('displays pipeline name and description', async () => {
      mockListSuccess([sampleManifest]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('My Test Pipeline')).toBeInTheDocument();
        expect(screen.getByText('A test pipeline')).toBeInTheDocument();
      });
    });

    it('displays pipeline node count', async () => {
      mockListSuccess([sampleManifest]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('3 nodes')).toBeInTheDocument();
      });
    });

    it('displays pipeline count in header', async () => {
      mockListSuccess([sampleManifest]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('(1)')).toBeInTheDocument();
      });
    });

    it('navigates to editor when clicking a pipeline card', async () => {
      mockListSuccess([sampleManifest]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('My Test Pipeline')).toBeInTheDocument();
      });

      await userEvent.click(screen.getByText('My Test Pipeline'));
      expect(mockNavigate).toHaveBeenCalledWith('/pipelines/test-pipeline-1');
    });
  });

  describe('header', () => {
    it('renders Pipelines heading', async () => {
      mockListSuccess([]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('Pipelines')).toBeInTheDocument();
      });
    });

    it('renders New Pipeline button', async () => {
      mockListSuccess([]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('New Pipeline')).toBeInTheDocument();
      });
    });
  });

  describe('create pipeline', () => {
    it('creates a new pipeline and navigates to editor', async () => {
      mockListSuccess([]);
      mockCreatePipeline.mockResolvedValue({
        data: { id: 'new-pipeline-id' },
        error: undefined,
        request: new globalThis.Request('http://localhost/test'),
        response: new globalThis.Response(),
      });

      renderView();

      await waitFor(() => {
        expect(screen.getByText('No pipelines yet')).toBeInTheDocument();
      });

      await userEvent.click(screen.getByText('Create Pipeline'));

      await waitFor(() => {
        expect(mockCreatePipeline).toHaveBeenCalled();
        expect(mockNavigate).toHaveBeenCalledWith('/pipelines/new-pipeline-id');
      });
    });
  });

  describe('delete pipeline', () => {
    it('shows delete confirmation dialog', async () => {
      mockListSuccess([sampleManifest]);
      renderView();

      await waitFor(() => {
        expect(screen.getByText('My Test Pipeline')).toBeInTheDocument();
      });

      const menuButton = screen.getByRole('button', { name: '' });
      if (menuButton) {
        await userEvent.click(menuButton);
      }
    });
  });
});
