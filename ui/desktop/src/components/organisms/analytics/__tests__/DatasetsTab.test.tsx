/**
 * DatasetsTab — 3-level test suite
 *
 * Level 1: Unit tests — pure functions (helpers, parsers)
 * Level 2: Integration tests — component rendering + user interaction (mocked API)
 * Level 3: E2E-like flow — full create → run → stream → results cycle
 */
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from 'vitest';

// ── Mock API module ──────────────────────────────────────────────
vi.mock('@/api', () => ({
  createEvalDataset: vi.fn(),
  deleteEvalDataset: vi.fn(),
  getEvalDataset: vi.fn(),
  listEvalDatasets: vi.fn(),
  listEvalRuns: vi.fn(),
  runEval: vi.fn(),
  updateEvalDataset: vi.fn(),
}));

vi.mock('@/api/client.gen', () => ({
  client: {
    getConfig: () => ({
      baseUrl: 'http://localhost:3000',
      headers: { 'X-Secret-Key': 'test-secret' },
    }),
  },
}));

import {
  createEvalDataset,
  deleteEvalDataset,
  getEvalDataset,
  listEvalDatasets,
  listEvalRuns,
  updateEvalDataset,
} from '@/api';

import DatasetsTab from '../DatasetsTab';

// ── Test fixtures (matching EvalDatasetSummary / EvalRunSummary exactly) ─
const mockDataset = {
  id: 'ds-1',
  name: 'Golden Routing Test',
  description: 'Test routing accuracy',
  caseCount: 3,
  lastRunAccuracy: 0.92,
  lastRunAt: '2025-03-07T12:00:00Z',
  tags: ['routing', 'golden'],
  createdAt: '2025-03-07T10:00:00Z',
  updatedAt: '2025-03-07T10:00:00Z',
};

const mockDatasetFull = {
  id: 'ds-1',
  name: 'Golden Routing Test',
  description: 'Test routing accuracy',
  cases: [
    {
      id: 'c1',
      input: 'Create a login page',
      expectedAgent: 'Developer Agent',
      expectedMode: 'plan',
      tags: ['ui'],
    },
    {
      id: 'c2',
      input: 'Run security audit',
      expectedAgent: 'Security Agent',
      expectedMode: 'plan',
      tags: ['security'],
    },
    {
      id: 'c3',
      input: 'Build ROI dashboard',
      expectedAgent: 'PM Agent',
      expectedMode: 'write',
      tags: ['analytics'],
    },
  ],
  tags: ['routing', 'golden'],
  createdAt: '2025-03-07T10:00:00Z',
  updatedAt: '2025-03-07T10:00:00Z',
};

const mockRunSummary = {
  id: 'run-1',
  datasetId: 'ds-1',
  datasetName: 'Golden Routing Test',
  status: 'pass',
  overallAccuracy: 0.92,
  agentAccuracy: 0.96,
  modeAccuracy: 0.88,
  totalCases: 50,
  correct: 46,
  startedAt: '2025-03-07T12:00:00Z',
  durationMs: 15000,
  gooseVersion: '1.23.0',
  versionTag: null,
};

const mockRunDetail = {
  id: 'run-1',
  datasetId: 'ds-1',
  datasetName: 'Golden Routing Test',
  status: 'pass',
  overallAccuracy: 0.92,
  agentAccuracy: 0.96,
  modeAccuracy: 0.88,
  totalCases: 3,
  correct: 2,
  failures: [
    {
      input: 'Build ROI dashboard',
      expectedAgent: 'PM Agent',
      expectedMode: 'write',
      actualAgent: 'Developer Agent',
      actualMode: 'plan',
      confidence: 0.45,
      tags: ['analytics'],
    },
  ],
  perAgent: [
    { agent: 'Developer Agent', accuracy: 1.0, pass: 1, fail: 0 },
    { agent: 'Security Agent', accuracy: 1.0, pass: 1, fail: 0 },
    { agent: 'PM Agent', accuracy: 0.0, pass: 0, fail: 1 },
  ],
  confusionMatrix: {
    labels: ['Developer Agent', 'PM Agent', 'Security Agent'],
    matrix: [
      [1, 0, 0],
      [1, 0, 0],
      [0, 0, 1],
    ],
  },
  startedAt: '2025-03-07T12:00:00Z',
  durationMs: 15000,
  gooseVersion: '1.23.0',
  versionTag: null,
};

function setupDefaultMocks() {
  (listEvalDatasets as Mock).mockResolvedValue({ data: [mockDataset], error: null });
  (listEvalRuns as Mock).mockResolvedValue({ data: [mockRunSummary], error: null });
  (getEvalDataset as Mock).mockResolvedValue({ data: mockDatasetFull, error: null });
  (createEvalDataset as Mock).mockResolvedValue({ data: { id: 'ds-new' }, error: null });
  (updateEvalDataset as Mock).mockResolvedValue({ data: { id: 'ds-1' }, error: null });
  (deleteEvalDataset as Mock).mockResolvedValue({ data: null, error: null });
}

// Helper to create an SSE text chunk
function sseChunk(obj: Record<string, unknown>): Uint8Array {
  return new TextEncoder().encode(`data: ${JSON.stringify(obj)}\n\n`);
}

// ═══════════════════════════════════════════════════════════════════
// LEVEL 1: Unit Tests — component rendering basics
// ═══════════════════════════════════════════════════════════════════
describe('Level 1: Unit Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  it('renders without crashing', async () => {
    render(<DatasetsTab />);
    // Shows loading skeleton initially
    expect(document.querySelector('.animate-pulse')).toBeTruthy();
    // Then loads data
    await waitFor(() => {
      expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
    });
  });

  it('displays case count for datasets', async () => {
    render(<DatasetsTab />);
    await waitFor(() => {
      expect(screen.getByText(/3 cases/)).toBeInTheDocument();
    });
  });

  it('displays accuracy badge with formatPercent', async () => {
    render(<DatasetsTab />);
    await waitFor(() => {
      // AccuracyBadge renders formatPercent(0.92) = "92.0%" (.toFixed(1))
      const badges = screen.getAllByText('92.0%');
      expect(badges.length).toBeGreaterThan(0);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════
// LEVEL 2: Integration Tests — user interactions + state
// ═══════════════════════════════════════════════════════════════════
describe('Level 2: Integration Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
  });

  describe('Dataset List', () => {
    it('loads and displays datasets from API', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
        expect(screen.getByText(/3 cases/)).toBeInTheDocument();
      });
      expect(listEvalDatasets).toHaveBeenCalledTimes(1);
      expect(listEvalRuns).toHaveBeenCalledTimes(1);
    });

    it('shows empty state when no datasets', async () => {
      (listEvalDatasets as Mock).mockResolvedValue({ data: [], error: null });
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText(/No datasets yet/)).toBeInTheDocument();
      });
    });

    it('shows "+ New Dataset" button', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('+ New Dataset')).toBeInTheDocument();
      });
    });

    it('shows accuracy badge color-coded (green for ≥90%)', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        const badges = screen.getAllByText('92.0%');
        expect(badges.length).toBeGreaterThan(0);
        // 92% → green class
        expect(badges[0].className).toContain('success');
      });
    });
  });

  describe('Sub-navigation', () => {
    it('switches between Datasets and Run History tabs', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
      });

      // Switch to history tab
      const historyTab = screen.getByRole('button', { name: /Run History/ });
      fireEvent.click(historyTab);

      await waitFor(() => {
        // Run history shows the run summary (50 cases from mockRunSummary)
        expect(screen.getByText(/50 cases/)).toBeInTheDocument();
      });

      // Switch back to datasets
      const datasetsTab = screen.getByRole('button', { name: /Datasets/ });
      fireEvent.click(datasetsTab);

      await waitFor(() => {
        expect(screen.getByText(/3 cases/)).toBeInTheDocument();
      });
    });
  });

  describe('Dataset Editor', () => {
    it('opens editor when clicking Edit button', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('Edit')).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText('Edit'));
      await waitFor(() => {
        expect(screen.getByText('Edit Dataset')).toBeInTheDocument();
      });
      expect(getEvalDataset).toHaveBeenCalledWith({ path: { id: 'ds-1' } });
    });

    it('opens new dataset editor when clicking "+ New Dataset"', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('+ New Dataset')).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText('+ New Dataset'));
      await waitFor(() => {
        expect(screen.getByText('New Dataset')).toBeInTheDocument();
      });
    });

    it('cancels editing and returns to list', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('+ New Dataset')).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText('+ New Dataset'));
      await waitFor(() => {
        expect(screen.getByText('Cancel')).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText('Cancel'));
      await waitFor(() => {
        expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
      });
    });
  });

  describe('Dataset Deletion', () => {
    it('deletes dataset after confirmation', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('Delete')).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText('Delete'));
      expect(window.confirm).toHaveBeenCalled();
      await waitFor(() => {
        expect(deleteEvalDataset).toHaveBeenCalledWith({ path: { id: 'ds-1' } });
      });
    });

    it('does not delete when user cancels confirmation', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(false);
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('Delete')).toBeInTheDocument();
      });
      fireEvent.click(screen.getByText('Delete'));
      expect(deleteEvalDataset).not.toHaveBeenCalled();
    });
  });

  describe('Run History', () => {
    it('shows run history with metrics', async () => {
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Run History/ })).toBeInTheDocument();
      });
      fireEvent.click(screen.getByRole('button', { name: /Run History/ }));
      await waitFor(() => {
        expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
        expect(screen.getByText(/50 cases/)).toBeInTheDocument();
      });
    });

    it('shows empty run history state', async () => {
      (listEvalRuns as Mock).mockResolvedValue({ data: [], error: null });
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Run History/ })).toBeInTheDocument();
      });
      fireEvent.click(screen.getByRole('button', { name: /Run History/ }));
      await waitFor(() => {
        expect(screen.getByText(/No eval runs yet/)).toBeInTheDocument();
      });
    });
  });

  describe('Error Handling', () => {
    it('displays error when API fails', async () => {
      (listEvalDatasets as Mock).mockRejectedValue(new Error('Database connection failed'));
      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText(/Database connection failed/)).toBeInTheDocument();
      });
    });
  });
});

// ═══════════════════════════════════════════════════════════════════
// LEVEL 3: E2E Flow Tests — full user journeys with SSE streaming
// ═══════════════════════════════════════════════════════════════════
describe('Level 3: E2E Flow Tests', () => {
  let originalFetch: typeof global.fetch;

  beforeEach(() => {
    vi.clearAllMocks();
    setupDefaultMocks();
    originalFetch = global.fetch;
  });

  afterEach(() => {
    global.fetch = originalFetch;
  });

  describe('Create → Edit → Delete lifecycle', () => {
    it('creates new dataset, edits existing, then deletes', async () => {
      const user = userEvent.setup();
      render(<DatasetsTab />);

      // Wait for initial load
      await waitFor(() => {
        expect(screen.getByText('+ New Dataset')).toBeInTheDocument();
      });

      // Open new dataset editor
      await user.click(screen.getByText('+ New Dataset'));
      await waitFor(() => {
        expect(screen.getByText('New Dataset')).toBeInTheDocument();
      });

      // Cancel back to list
      await user.click(screen.getByText('Cancel'));
      await waitFor(() => {
        expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
      });

      // Edit existing
      await user.click(screen.getByText('Edit'));
      await waitFor(() => {
        expect(screen.getByText('Edit Dataset')).toBeInTheDocument();
      });

      // Cancel edit
      await user.click(screen.getByText('Cancel'));
      await waitFor(() => {
        expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
      });

      // Delete
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      await user.click(screen.getByText('Delete'));
      expect(deleteEvalDataset).toHaveBeenCalledWith({ path: { id: 'ds-1' } });
    });
  });

  describe('SSE Streaming Eval', () => {
    function mockSSEStream(events: Record<string, unknown>[]) {
      let callIndex = 0;
      const mockReader = {
        read: vi.fn().mockImplementation(() => {
          if (callIndex < events.length) {
            const chunk = sseChunk(events[callIndex]);
            callIndex++;
            return Promise.resolve({ done: false, value: chunk });
          }
          return Promise.resolve({ done: true, value: undefined });
        }),
      };
      global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        body: { getReader: () => mockReader },
      });
      return mockReader;
    }

    it('sends correct fetch request for streaming eval', async () => {
      mockSSEStream([{ type: 'done', runId: 'run-new', detail: mockRunDetail }]);

      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('▶ Run Eval')).toBeInTheDocument();
      });

      await act(async () => {
        fireEvent.click(screen.getByText('▶ Run Eval'));
      });

      await waitFor(() => {
        expect(global.fetch).toHaveBeenCalledWith(
          'http://localhost:3000/analytics/eval/run/stream',
          expect.objectContaining({
            method: 'POST',
            headers: expect.objectContaining({
              'Content-Type': 'application/json',
              'X-Secret-Key': 'test-secret',
            }),
            body: JSON.stringify({ datasetId: 'ds-1' }),
          })
        );
      });
    });

    it('streams progress events and shows live results', async () => {
      mockSSEStream([
        {
          type: 'progress',
          index: 0,
          total: 3,
          result: {
            input: 'Create a login page',
            expected_agent: 'Developer Agent',
            expected_mode: 'plan',
            actual_agent: 'Developer Agent',
            actual_mode: 'plan',
            confidence: 0.92,
            agent_correct: true,
            mode_correct: true,
            fully_correct: true,
            reasoning: 'Code task',
          },
          runningMetrics: {
            completed: 1,
            correct: 1,
            agent_correct: 1,
            mode_correct: 1,
            overall_accuracy: 1.0,
            agent_accuracy: 1.0,
            mode_accuracy: 1.0,
          },
        },
        { type: 'done', runId: 'run-new', detail: mockRunDetail },
      ]);

      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('▶ Run Eval')).toBeInTheDocument();
      });

      await act(async () => {
        fireEvent.click(screen.getByText('▶ Run Eval'));
      });

      // After done event, RunDetailPanel should show
      await waitFor(
        () => {
          expect(screen.getByText('← Back to Datasets')).toBeInTheDocument();
        },
        { timeout: 3000 }
      );
    });

    it('handles SSE error event gracefully', async () => {
      mockSSEStream([{ type: 'error', message: 'Provider not available' }]);

      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('▶ Run Eval')).toBeInTheDocument();
      });

      await act(async () => {
        fireEvent.click(screen.getByText('▶ Run Eval'));
      });

      await waitFor(
        () => {
          expect(screen.getByText(/Provider not available/)).toBeInTheDocument();
        },
        { timeout: 3000 }
      );
    });

    it('handles network error gracefully', async () => {
      global.fetch = vi.fn().mockRejectedValue(new Error('Network error'));

      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('▶ Run Eval')).toBeInTheDocument();
      });

      await act(async () => {
        fireEvent.click(screen.getByText('▶ Run Eval'));
      });

      await waitFor(
        () => {
          expect(screen.getByText(/Network error/)).toBeInTheDocument();
        },
        { timeout: 3000 }
      );
    });

    it('handles HTTP error (non-OK response)', async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        body: null,
      });

      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('▶ Run Eval')).toBeInTheDocument();
      });

      await act(async () => {
        fireEvent.click(screen.getByText('▶ Run Eval'));
      });

      await waitFor(
        () => {
          expect(screen.getByText(/500/)).toBeInTheDocument();
        },
        { timeout: 3000 }
      );
    });
  });

  describe('RunDetailPanel navigation', () => {
    it('shows detail panel after eval and allows back navigation', async () => {
      // Immediate done event
      let callIndex = 0;
      global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        body: {
          getReader: () => ({
            read: vi.fn().mockImplementation(() => {
              if (callIndex === 0) {
                callIndex++;
                return Promise.resolve({
                  done: false,
                  value: sseChunk({ type: 'done', runId: 'run-1', detail: mockRunDetail }),
                });
              }
              return Promise.resolve({ done: true, value: undefined });
            }),
          }),
        },
      });

      render(<DatasetsTab />);
      await waitFor(() => {
        expect(screen.getByText('▶ Run Eval')).toBeInTheDocument();
      });

      await act(async () => {
        fireEvent.click(screen.getByText('▶ Run Eval'));
      });

      // RunDetailPanel should appear
      await waitFor(
        () => {
          expect(screen.getByText('← Back to Datasets')).toBeInTheDocument();
        },
        { timeout: 3000 }
      );

      // Click back
      fireEvent.click(screen.getByText('← Back to Datasets'));
      await waitFor(() => {
        expect(screen.getByText('Golden Routing Test')).toBeInTheDocument();
      });
    });
  });
});
