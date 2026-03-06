import { useCallback, useEffect, useMemo, useState } from 'react';
import type { CreateDatasetRequest, EvalDataset, EvalDatasetSummary } from '@/api';
import {
  createEvalDataset,
  deleteEvalDataset,
  getEvalDataset,
  listEvalDatasets,
  listEvalRuns,
  runEval,
  updateEvalDataset,
} from '@/api';

// ── Types ──────────────────────────────────────────────────────────

interface TestCaseRow {
  id: string;
  input: string;
  expectedAgent: string;
  expectedMode: string;
  tags: string;
}

interface RunResult {
  input: string;
  expectedAgent: string;
  expectedMode: string;
  actualAgent: string;
  actualMode: string;
  confidence: number;
  agentCorrect: boolean;
  modeCorrect: boolean;
  fullyCorrect: boolean;
}

interface RunSummary {
  id: string;
  datasetId: string;
  datasetName: string;
  status: string;
  accuracy: number;
  agentAccuracy: number;
  modeAccuracy: number;
  totalCases: number;
  createdAt: string;
}

// ── Helpers ────────────────────────────────────────────────────────

function newRowId(): string {
  return globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2);
}

function emptyRow(): TestCaseRow {
  return { id: newRowId(), input: '', expectedAgent: '', expectedMode: '', tags: '' };
}

function formatPercent(v: number): string {
  return `${(v * 100).toFixed(1)}%`;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function AccuracyBadge({ value }: { value: number }) {
  const color =
    value >= 0.9
      ? 'bg-background-success-muted text-text-success border-border-default'
      : value >= 0.7
        ? 'bg-background-warning-muted text-text-warning border-border-default'
        : 'bg-background-danger-muted text-text-danger border-border-default';
  return (
    <span className={`text-xs px-2 py-0.5 rounded-full border ${color}`}>
      {formatPercent(value)}
    </span>
  );
}

function StatusBadge({ status }: { status: string }) {
  const color =
    status === 'pass'
      ? 'text-text-success'
      : status === 'degraded'
        ? 'text-text-warning'
        : 'text-text-danger';
  return <span className={`text-xs font-medium ${color}`}>{status.toUpperCase()}</span>;
}

// ── ConfusionMatrix ────────────────────────────────────────────────

function ConfusionMatrix({ results }: { results: RunResult[] }) {
  const matrix = useMemo(() => {
    const agents = new Set<string>();
    for (const r of results) {
      agents.add(r.expectedAgent);
      agents.add(r.actualAgent);
    }
    const sorted = [...agents].sort();
    const counts: Record<string, Record<string, number>> = {};
    for (const expected of sorted) {
      counts[expected] = {};
      for (const actual of sorted) {
        counts[expected][actual] = 0;
      }
    }
    for (const r of results) {
      if (counts[r.expectedAgent]) {
        counts[r.expectedAgent][r.actualAgent] = (counts[r.expectedAgent][r.actualAgent] || 0) + 1;
      }
    }
    return { agents: sorted, counts };
  }, [results]);

  if (matrix.agents.length === 0) return null;

  return (
    <div className="overflow-x-auto">
      <table className="text-xs border-collapse">
        <thead>
          <tr>
            <th className="p-2 text-text-muted font-normal text-left">Expected ↓ / Actual →</th>
            {matrix.agents.map((a) => (
              <th key={a} className="p-2 text-text-muted font-normal text-center whitespace-nowrap">
                {a}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {matrix.agents.map((expected) => (
            <tr key={expected}>
              <td className="p-2 text-text-default font-medium whitespace-nowrap">{expected}</td>
              {matrix.agents.map((actual) => {
                const count = matrix.counts[expected]?.[actual] || 0;
                const isDiagonal = expected === actual;
                const bg =
                  count === 0
                    ? ''
                    : isDiagonal
                      ? 'bg-background-success-muted'
                      : 'bg-background-danger-muted';
                return (
                  <td key={actual} className={`p-2 text-center border border-border-default ${bg}`}>
                    {count > 0 ? count : '·'}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── RunDetailPanel ─────────────────────────────────────────────────

function RunDetailPanel({
  results,
  metrics,
  onClose,
}: {
  results: RunResult[];
  metrics: { accuracy: number; agentAccuracy: number; modeAccuracy: number; total: number };
  onClose: () => void;
}) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-semibold text-text-default">Run Results</h4>
        <button
          type="button"
          onClick={onClose}
          className="text-xs text-text-muted hover:text-text-default"
        >
          ✕ Close
        </button>
      </div>

      {/* KPI row */}
      <div className="grid grid-cols-4 gap-3">
        <div className="rounded-lg bg-background-muted p-3 text-center">
          <div className="text-lg font-bold text-text-default">
            {formatPercent(metrics.accuracy)}
          </div>
          <div className="text-xs text-text-muted">Overall</div>
        </div>
        <div className="rounded-lg bg-background-muted p-3 text-center">
          <div className="text-lg font-bold text-text-default">
            {formatPercent(metrics.agentAccuracy)}
          </div>
          <div className="text-xs text-text-muted">Agent</div>
        </div>
        <div className="rounded-lg bg-background-muted p-3 text-center">
          <div className="text-lg font-bold text-text-default">
            {formatPercent(metrics.modeAccuracy)}
          </div>
          <div className="text-xs text-text-muted">Mode</div>
        </div>
        <div className="rounded-lg bg-background-muted p-3 text-center">
          <div className="text-lg font-bold text-text-default">{metrics.total}</div>
          <div className="text-xs text-text-muted">Cases</div>
        </div>
      </div>

      {/* Confusion matrix */}
      <div>
        <h5 className="text-xs font-medium text-text-muted mb-2">Confusion Matrix</h5>
        <ConfusionMatrix results={results} />
      </div>

      {/* Results table */}
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="border-b border-border-default">
              <th className="text-left p-2 text-text-muted font-normal">Status</th>
              <th className="text-left p-2 text-text-muted font-normal">Input</th>
              <th className="text-left p-2 text-text-muted font-normal">Expected</th>
              <th className="text-left p-2 text-text-muted font-normal">Actual</th>
              <th className="text-right p-2 text-text-muted font-normal">Conf.</th>
            </tr>
          </thead>
          <tbody>
            {results.map((r, i) => (
              <tr
                key={`result-${r.input.slice(0, 20)}-${i}`}
                className="border-b border-border-default hover:bg-background-muted"
              >
                <td className="p-2">
                  {r.fullyCorrect ? (
                    <span className="text-text-success">✓</span>
                  ) : r.agentCorrect ? (
                    <span className="text-text-warning">◐</span>
                  ) : (
                    <span className="text-text-danger">✗</span>
                  )}
                </td>
                <td className="p-2 text-text-default max-w-[300px] truncate" title={r.input}>
                  {r.input}
                </td>
                <td className="p-2 text-text-muted whitespace-nowrap">
                  {r.expectedAgent} / {r.expectedMode}
                </td>
                <td className="p-2 text-text-default whitespace-nowrap">
                  {r.actualAgent} / {r.actualMode}
                </td>
                <td className="p-2 text-right text-text-muted">{formatPercent(r.confidence)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ── DatasetEditor ──────────────────────────────────────────────────

function DatasetEditor({
  dataset,
  onSave,
  onCancel,
}: {
  dataset: EvalDataset | null;
  onSave: (req: CreateDatasetRequest) => Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState(dataset?.name ?? '');
  const [description, setDescription] = useState(dataset?.description ?? '');
  const [rows, setRows] = useState<TestCaseRow[]>(() => {
    if (dataset?.cases && dataset.cases.length > 0) {
      return dataset.cases.map((tc) => ({
        id: tc.id,
        input: tc.input,
        expectedAgent: tc.expectedAgent,
        expectedMode: tc.expectedMode,
        tags: (tc.tags ?? []).join(', '),
      }));
    }
    return [emptyRow()];
  });
  const [saving, setSaving] = useState(false);
  const [yamlMode, setYamlMode] = useState(false);
  const [yamlText, setYamlText] = useState('');

  // Sync state when dataset prop changes (e.g., switching from "new" to editing existing)
  useEffect(() => {
    setName(dataset?.name ?? '');
    setDescription(dataset?.description ?? '');
    if (dataset?.cases && dataset.cases.length > 0) {
      setRows(
        dataset.cases.map((tc) => ({
          id: tc.id,
          input: tc.input,
          expectedAgent: tc.expectedAgent,
          expectedMode: tc.expectedMode,
          tags: (tc.tags ?? []).join(', '),
        }))
      );
    } else {
      setRows([emptyRow()]);
    }
    setYamlMode(false);
    setYamlText('');
  }, [dataset]);

  const addRow = () => setRows([...rows, emptyRow()]);

  const removeRow = (idx: number) => {
    if (rows.length <= 1) return;
    setRows(rows.filter((_, i) => i !== idx));
  };

  const updateRow = (idx: number, field: keyof TestCaseRow, value: string) => {
    const updated = [...rows];
    updated[idx] = { ...updated[idx], [field]: value };
    setRows(updated);
  };

  const parseYamlToCases = (
    yaml: string,
  ): Array<{
    input: string;
    expectedAgent: string;
    expectedMode: string;
    tags: string[];
  }> => {
    try {
      // Try parsing as object with test_cases key
      let parsed: unknown;
      try {
        parsed = JSON.parse(
          JSON.stringify(
            // Simple YAML parsing: extract entries manually
            null,
          ),
        );
      } catch {
        // ignore
      }
      // Use regex-based extraction for YAML since we don't have a YAML parser in the browser
      const entries: Array<{
        input: string;
        expectedAgent: string;
        expectedMode: string;
        tags: string[];
      }> = [];
      // Split on "- input:" to get individual entries
      const blocks = yaml.split(/(?=- input:)/);
      for (const block of blocks) {
        const inputMatch = block.match(
          /- input:\s*"?([^"\n]+)"?/,
        );
        const agentMatch = block.match(
          /expected_agent:\s*"?([^"\n]+)"?/,
        );
        const modeMatch = block.match(
          /expected_mode:\s*"?([^"\n]+)"?/,
        );
        const tagsMatch = block.match(
          /tags:\s*\[([^\]]*)\]/,
        );
        if (inputMatch) {
          entries.push({
            input: inputMatch[1].trim(),
            expectedAgent: agentMatch ? agentMatch[1].trim() : '',
            expectedMode: modeMatch ? modeMatch[1].trim() : '',
            tags: tagsMatch
              ? tagsMatch[1]
                  .split(',')
                  .map((t) => t.trim().replace(/"/g, ''))
                  .filter(Boolean)
              : [],
          });
        }
      }
      return entries;
    } catch {
      return [];
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      let cases: Array<{
        input: string;
        expectedAgent: string;
        expectedMode: string;
        tags: string[];
      }>;

      if (yamlMode && yamlText.trim()) {
        // Parse YAML text back into cases
        cases = parseYamlToCases(yamlText);
      } else {
        cases = rows
          .filter((r) => r.input.trim())
          .map((r) => ({
            input: r.input.trim(),
            expectedAgent: r.expectedAgent.trim(),
            expectedMode: r.expectedMode.trim(),
            tags: r.tags
              .split(',')
              .map((t) => t.trim())
              .filter(Boolean),
          }));
      }
      await onSave({ name, description, cases });
    } finally {
      setSaving(false);
    }
  };

  const generateYaml = () => {
    const cases = rows
      .filter((r) => r.input.trim())
      .map((r) => {
        let yaml = `  - input: "${r.input.replace(/"/g, '\\"')}"\n`;
        yaml += `    expected_agent: "${r.expectedAgent}"\n`;
        yaml += `    expected_mode: "${r.expectedMode}"`;
        const tags = r.tags
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean);
        if (tags.length > 0) {
          yaml += `\n    tags: [${tags.map((t) => `"${t}"`).join(', ')}]`;
        }
        return yaml;
      });
    setYamlText(`test_cases:\n${cases.join('\n')}`);
    setYamlMode(true);
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-text-default">
          {dataset ? 'Edit Dataset' : 'New Dataset'}
        </h3>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => {
              if (yamlMode) {
                // Parse YAML back to table rows
                const parsed = parseYamlToCases(yamlText);
                if (parsed.length > 0) {
                  setRows(
                    parsed.map((c) => ({
                      id: `row-${Date.now()}-${Math.random()}`,
                      input: c.input,
                      expectedAgent: c.expectedAgent,
                      expectedMode: c.expectedMode,
                      tags: c.tags.join(', '),
                    })),
                  );
                }
                setYamlMode(false);
              } else {
                generateYaml();
              }
            }}
            className="px-3 py-1.5 rounded border border-border-default text-text-default text-xs hover:bg-background-muted transition-colors"
          >
            {yamlMode ? 'Table View' : 'YAML View'}
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="px-3 py-1.5 rounded border border-border-default text-text-default text-xs hover:bg-background-muted transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={saving || !name.trim()}
            className="px-4 py-1.5 rounded-lg bg-background-accent text-text-on-accent text-xs font-medium disabled:opacity-50 transition-colors"
          >
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="block text-xs text-text-muted mb-1">Name</label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g., Core Routing Tests"
            className="w-full px-3 py-2 rounded-lg border border-border-default bg-background-default text-text-default text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>
        <div>
          <label className="block text-xs text-text-muted mb-1">Description</label>
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Optional description"
            className="w-full px-3 py-2 rounded-lg border border-border-default bg-background-default text-text-default text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>
      </div>

      {yamlMode ? (
        <textarea
          value={yamlText}
          onChange={(e) => setYamlText(e.target.value)}
          rows={15}
          className="w-full px-3 py-2 rounded-lg border border-border-default bg-background-default text-text-default font-mono text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      ) : (
        <div className="space-y-2">
          <div className="grid grid-cols-[1fr_150px_120px_120px_32px] gap-2 text-xs text-text-muted font-medium px-1">
            <span>Input Message</span>
            <span>Expected Agent</span>
            <span>Expected Mode</span>
            <span>Tags</span>
            <span />
          </div>
          {rows.map((row, idx) => (
            <div key={row.id} className="grid grid-cols-[1fr_150px_120px_120px_32px] gap-2">
              <input
                value={row.input}
                onChange={(e) => updateRow(idx, 'input', e.target.value)}
                placeholder="User message..."
                className="px-2 py-1.5 rounded border border-border-default bg-background-default text-text-default text-xs focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
              <input
                value={row.expectedAgent}
                onChange={(e) => updateRow(idx, 'expectedAgent', e.target.value)}
                placeholder="Agent name"
                className="px-2 py-1.5 rounded border border-border-default bg-background-default text-text-default text-xs focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
              <input
                value={row.expectedMode}
                onChange={(e) => updateRow(idx, 'expectedMode', e.target.value)}
                placeholder="Mode"
                className="px-2 py-1.5 rounded border border-border-default bg-background-default text-text-default text-xs focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
              <input
                value={row.tags}
                onChange={(e) => updateRow(idx, 'tags', e.target.value)}
                placeholder="tag1, tag2"
                className="px-2 py-1.5 rounded border border-border-default bg-background-default text-text-default text-xs focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
              <button
                type="button"
                onClick={() => removeRow(idx)}
                className="text-text-muted hover:text-text-danger text-xs"
                title="Remove row"
              >
                ✕
              </button>
            </div>
          ))}
          <button
            type="button"
            onClick={addRow}
            className="text-xs text-text-muted hover:text-text-default"
          >
            + Add row
          </button>
        </div>
      )}
    </div>
  );
}

// ── Main Component ─────────────────────────────────────────────────

export default function DatasetsTab() {
  const [datasets, setDatasets] = useState<EvalDatasetSummary[]>([]);
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [editDataset, setEditDataset] = useState<EvalDataset | null>(null);
  const [runningDataset, setRunningDataset] = useState<string | null>(null);
  const [activeRunResults, setActiveRunResults] = useState<{
    results: RunResult[];
    metrics: { accuracy: number; agentAccuracy: number; modeAccuracy: number; total: number };
  } | null>(null);
  const [view, setView] = useState<'datasets' | 'history'>('datasets');

  const fetchAll = useCallback(async () => {
    setLoading(true);
    try {
      const [dsResp, runsResp] = await Promise.all([listEvalDatasets(), listEvalRuns()]);
      setDatasets((dsResp.data as EvalDatasetSummary[]) ?? []);
      setRuns((runsResp.data as RunSummary[]) ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load data');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchAll();
  }, [fetchAll]);

  const handleEdit = async (id: string) => {
    try {
      const resp = await getEvalDataset({ path: { id } });
      setEditDataset(resp.data as EvalDataset);
      setEditing(id);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load dataset');
    }
  };

  const handleSave = async (req: CreateDatasetRequest) => {
    if (editing === 'new') {
      await createEvalDataset({ body: req });
    } else if (editing) {
      await updateEvalDataset({ path: { id: editing }, body: req });
    }
    setEditing(null);
    setEditDataset(null);
    await fetchAll();
  };

  const handleDelete = async (id: string) => {
    if (!confirm('Delete this dataset? This cannot be undone.')) return;
    await deleteEvalDataset({ path: { id } });
    await fetchAll();
  };

  const handleRunEval = async (datasetId: string) => {
    setRunningDataset(datasetId);
    setError(null);
    try {
      const resp = await runEval({ body: { datasetId } });
      const run = resp.data as {
        results: RunResult[];
        metrics: {
          overallAccuracy: number;
          agentAccuracy: number;
          modeAccuracy: number;
          total: number;
        };
      };
      setActiveRunResults({
        results: run.results ?? [],
        metrics: {
          accuracy: run.metrics?.overallAccuracy ?? 0,
          agentAccuracy: run.metrics?.agentAccuracy ?? 0,
          modeAccuracy: run.metrics?.modeAccuracy ?? 0,
          total: run.metrics?.total ?? 0,
        },
      });
      await fetchAll(); // refresh runs list
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Eval run failed');
    } finally {
      setRunningDataset(null);
    }
  };

  if (editing) {
    return (
      <DatasetEditor
        dataset={editing === 'new' ? null : editDataset}
        onSave={handleSave}
        onCancel={() => {
          setEditing(null);
          setEditDataset(null);
        }}
      />
    );
  }

  if (activeRunResults) {
    return (
      <RunDetailPanel
        results={activeRunResults.results}
        metrics={activeRunResults.metrics}
        onClose={() => setActiveRunResults(null)}
      />
    );
  }

  return (
    <div className="space-y-6">
      {error && (
        <div className="rounded-lg bg-background-danger-muted border border-border-default p-3 text-text-danger text-sm">
          {error}
        </div>
      )}

      {/* Sub-navigation: Datasets | Run History */}
      <div className="flex items-center justify-between">
        <div className="flex gap-1 rounded-lg bg-background-muted p-1">
          <button
            type="button"
            onClick={() => setView('datasets')}
            className={`px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              view === 'datasets'
                ? 'bg-background-default text-text-default shadow-sm'
                : 'text-text-muted hover:text-text-default'
            }`}
          >
            📋 Datasets ({datasets.length})
          </button>
          <button
            type="button"
            onClick={() => setView('history')}
            className={`px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              view === 'history'
                ? 'bg-background-default text-text-default shadow-sm'
                : 'text-text-muted hover:text-text-default'
            }`}
          >
            📜 Run History ({runs.length})
          </button>
        </div>
        {view === 'datasets' && (
          <button
            type="button"
            onClick={() => setEditing('new')}
            className="px-4 py-2 rounded-lg bg-background-accent hover:bg-background-accent text-text-on-accent text-sm font-medium transition-colors"
          >
            + New Dataset
          </button>
        )}
      </div>

      {loading ? (
        <div className="space-y-3 animate-pulse">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={`skeleton-${i + 1}`} className="h-16 rounded-lg bg-background-muted" />
          ))}
        </div>
      ) : view === 'datasets' ? (
        /* ── Datasets View ── */
        datasets.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-text-muted">
            <p className="text-lg mb-2">No datasets yet</p>
            <p className="text-sm mb-4">
              Create a dataset to define golden test cases for evaluating routing accuracy.
            </p>
            <button
              type="button"
              onClick={() => setEditing('new')}
              className="px-4 py-2 rounded-lg bg-background-accent hover:bg-background-accent text-text-on-accent text-sm font-medium transition-colors"
            >
              Create First Dataset
            </button>
          </div>
        ) : (
          <div className="space-y-2">
            {datasets.map((ds) => (
              <div
                key={ds.id}
                className="rounded-lg border border-border-default bg-background-muted p-4 flex items-center justify-between hover:border-border-default transition-colors"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-3">
                    <h4 className="text-text-default font-medium">{ds.name}</h4>
                    <span className="text-xs bg-background-muted text-text-default px-2 py-0.5 rounded-full border border-border-default">
                      {ds.caseCount} cases
                    </span>
                    {ds.lastRunAccuracy != null && <AccuracyBadge value={ds.lastRunAccuracy} />}
                  </div>
                  {ds.description && (
                    <p className="text-sm text-text-muted mt-1 truncate">{ds.description}</p>
                  )}
                  <p className="text-xs text-text-muted mt-1">
                    Updated {new Date(ds.updatedAt).toLocaleDateString()}
                  </p>
                </div>
                <div className="flex gap-2 ml-4">
                  <button
                    type="button"
                    onClick={() => handleRunEval(ds.id)}
                    disabled={runningDataset === ds.id || ds.caseCount === 0}
                    className="px-3 py-1.5 rounded bg-background-accent text-text-on-accent text-xs font-medium disabled:opacity-50 transition-colors"
                  >
                    {runningDataset === ds.id ? '⏳ Running...' : '▶ Run Eval'}
                  </button>
                  <button
                    type="button"
                    onClick={() => handleEdit(ds.id)}
                    className="px-3 py-1.5 rounded border border-border-default text-text-default text-xs hover:bg-background-muted transition-colors"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDelete(ds.id)}
                    className="px-3 py-1.5 rounded border border-border-danger text-text-danger text-xs bg-background-default hover:bg-background-danger-muted transition-colors"
                  >
                    Delete
                  </button>
                </div>
              </div>
            ))}
          </div>
        )
      ) : /* ── Run History View ── */
      runs.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-48 text-text-muted">
          <p className="text-lg mb-2">No eval runs yet</p>
          <p className="text-sm mb-4">
            Create a dataset and click "Run Eval" to generate your first routing evaluation.
          </p>
          <button
            type="button"
            onClick={() => setView('datasets')}
            className="px-4 py-2 rounded-lg bg-background-accent hover:bg-background-accent text-text-on-accent text-sm font-medium transition-colors"
          >
            Go to Datasets
          </button>
        </div>
      ) : (
        <div className="space-y-2">
          {runs.map((run) => (
            <div
              key={run.id}
              className="rounded-lg border border-border-default bg-background-muted p-4 flex items-center justify-between hover:border-border-default transition-colors"
            >
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-3">
                  <StatusBadge status={run.status} />
                  <span className="text-text-default font-medium">{run.datasetName}</span>
                  <AccuracyBadge value={run.accuracy} />
                  <span className="text-xs text-text-muted">{run.totalCases} cases</span>
                </div>
                <div className="flex gap-4 mt-1 text-xs text-text-muted">
                  <span>Agent: {formatPercent(run.agentAccuracy)}</span>
                  <span>Mode: {formatPercent(run.modeAccuracy)}</span>
                  <span>{formatDate(run.createdAt)}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
