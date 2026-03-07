import { useCallback, useEffect, useState } from 'react';
import type {
  CreateDatasetRequest,
  EvalDataset,
  EvalDatasetSummary,
  EvalRunDetail,
  EvalRunSummary,
} from '@/api';
import {
  createEvalDataset,
  deleteEvalDataset,
  getEvalDataset,
  listEvalDatasets,
  listEvalRuns,
  updateEvalDataset,
} from '@/api';
import { client } from '@/api/client.gen';

// ── Types ──────────────────────────────────────────────────────────

interface TestCaseRow {
  id: string;
  input: string;
  expectedAgent: string;
  expectedMode: string;
  tags: string;
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

// ── RunDetailPanel ─────────────────────────────────────────────────

type RunDetailTab = 'overview' | 'matrix' | 'failures';

function kpiColor(value: number): string {
  if (value >= 0.9) return 'text-green-400';
  if (value >= 0.7) return 'text-yellow-400';
  return 'text-red-400';
}

function FailuresTable({ failures }: { failures: import('@/api').FailureDetail[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-xs">
        <thead>
          <tr className="border-b border-border-default">
            <th className="text-left p-2 text-text-muted font-normal">Input</th>
            <th className="text-left p-2 text-text-muted font-normal">Expected</th>
            <th className="text-left p-2 text-text-muted font-normal">Actual</th>
            <th className="text-right p-2 text-text-muted font-normal">Conf.</th>
            <th className="text-left p-2 text-text-muted font-normal">Reasoning</th>
          </tr>
        </thead>
        <tbody>
          {failures.map((f) => (
            <tr
              key={`fail-${f.expectedAgent}-${f.actualAgent}-${f.input.slice(0, 30)}`}
              className="border-b border-border-default hover:bg-background-muted"
            >
              <td className="p-2 text-text-default max-w-[300px] truncate" title={f.input}>
                {f.input}
              </td>
              <td className="p-2 text-text-muted whitespace-nowrap">
                {f.expectedAgent} / {f.expectedMode}
              </td>
              <td className="p-2 text-text-default whitespace-nowrap">
                <span className="text-red-400">{f.actualAgent}</span> / {f.actualMode}
              </td>
              <td className="p-2 text-right text-text-muted">{formatPercent(f.confidence)}</td>
              <td className="p-2 text-text-muted max-w-[250px]">
                <span className="text-xs italic">
                  {f.expectedAgent !== f.actualAgent
                    ? `Expected ${f.expectedAgent}, got ${f.actualAgent}`
                    : `Agent ✓, mode: expected ${f.expectedMode}, got ${f.actualMode}`}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ApiConfusionMatrix({ matrix }: { matrix: import('@/api').ConfusionMatrix }) {
  if (!matrix.labels.length) return null;

  return (
    <div className="overflow-x-auto">
      <table className="text-xs border-collapse">
        <thead>
          <tr>
            <th className="p-2 text-text-muted font-normal text-left">Expected ↓ / Actual →</th>
            {matrix.labels.map((label) => (
              <th
                key={label}
                className="p-2 text-text-muted font-normal text-center whitespace-nowrap"
              >
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {matrix.labels.map((expected, rowIdx) => (
            <tr key={expected}>
              <td className="p-2 text-text-default font-medium whitespace-nowrap">{expected}</td>
              {matrix.labels.map((actual, colIdx) => {
                const count = matrix.matrix[rowIdx]?.[colIdx] ?? 0;
                const isDiagonal = rowIdx === colIdx;
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

function PerAgentTable({ agents }: { agents: import('@/api').AgentResult[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-xs">
        <thead>
          <tr className="border-b border-border-default">
            <th className="text-left p-2 text-text-muted font-normal">Agent</th>
            <th className="text-right p-2 text-text-muted font-normal">Accuracy</th>
            <th className="text-right p-2 text-text-muted font-normal">Pass</th>
            <th className="text-right p-2 text-text-muted font-normal">Fail</th>
          </tr>
        </thead>
        <tbody>
          {agents.map((a) => (
            <tr key={a.agent} className="border-b border-border-default hover:bg-background-muted">
              <td className="p-2 text-text-default font-medium">{a.agent}</td>
              <td className={`p-2 text-right font-medium ${kpiColor(a.accuracy)}`}>
                {formatPercent(a.accuracy)}
              </td>
              <td className="p-2 text-right text-green-400">{a.pass}</td>
              <td className="p-2 text-right text-red-400">{a.fail}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

type StreamProgress = {
  index: number;
  total: number;
  results: Array<{
    input: string;
    expectedAgent: string;
    expectedMode: string;
    actualAgent: string;
    actualMode: string;
    confidence: number;
    agentCorrect: boolean;
    modeCorrect: boolean;
    fullyCorrect: boolean;
    reasoning: string;
  }>;
  metrics: { overallAccuracy: number; agentAccuracy: number; modeAccuracy: number };
};

function RunDetailPanel({
  runDetail,
  streamProgress,
  isRunning,
  onClose,
  onRerun,
  onEdit,
}: {
  runDetail: import('@/api').EvalRunDetail | null;
  streamProgress: StreamProgress | null;
  isRunning: boolean;
  onClose: () => void;
  onRerun?: () => void;
  onEdit?: () => void;
}) {
  const [tab, setTab] = useState<RunDetailTab>('overview');

  const isStreaming = isRunning && streamProgress !== null;
  const failureCount = isStreaming
    ? streamProgress.results.filter((r) => !r.fullyCorrect).length
    : (runDetail?.failures.length ?? 0);
  const passCount = isStreaming
    ? streamProgress.results.filter((r) => r.fullyCorrect).length
    : (runDetail?.totalCases ?? 0) - failureCount;
  const totalCases = isStreaming ? streamProgress.total : (runDetail?.totalCases ?? 0);
  const completedCases = isStreaming ? streamProgress.index : totalCases;

  const overallAcc = isStreaming
    ? streamProgress.metrics.overallAccuracy
    : (runDetail?.overallAccuracy ?? 0);
  const agentAcc = isStreaming
    ? streamProgress.metrics.agentAccuracy
    : (runDetail?.agentAccuracy ?? 0);
  const modeAcc = isStreaming
    ? streamProgress.metrics.modeAccuracy
    : (runDetail?.modeAccuracy ?? 0);

  const tabs: { key: RunDetailTab; label: string; count?: number }[] = [
    { key: 'overview', label: isStreaming ? 'Live Results' : 'Overview' },
    { key: 'matrix', label: 'Confusion Matrix' },
    { key: 'failures', label: 'Failures', count: failureCount },
  ];

  return (
    <div className="space-y-4">
      {/* Breadcrumb header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={onClose}
            className="text-sm text-text-muted hover:text-text-default transition-colors"
          >
            ← Back to Datasets
          </button>
          <span className="text-text-muted">/</span>
          <h4 className="text-sm font-semibold text-text-default">
            {runDetail?.datasetName ?? 'Running...'}
          </h4>
          {runDetail && (
            <span className="text-xs bg-background-muted text-text-muted px-2 py-0.5 rounded-full border border-border-default">
              {runDetail.totalCases} cases
            </span>
          )}
        </div>
        <div className="flex gap-2">
          {onEdit && (
            <button
              type="button"
              onClick={onEdit}
              className="px-3 py-1.5 rounded border border-border-default text-text-default text-xs hover:bg-background-muted transition-colors"
            >
              Edit Dataset
            </button>
          )}
          {onRerun && (
            <button
              type="button"
              onClick={onRerun}
              disabled={isRunning}
              className="px-3 py-1.5 rounded bg-background-accent text-text-on-accent text-xs font-medium disabled:opacity-50 transition-colors"
            >
              {isRunning ? '⏳ Running...' : '▶ Re-run'}
            </button>
          )}
        </div>
      </div>

      {/* Progress bar (shown while streaming) */}
      {isStreaming && (
        <div className="rounded-lg border border-border-default bg-background-muted p-3">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs text-text-muted">
              Evaluating case {completedCases} of {totalCases}...
            </span>
            <span className="text-xs font-mono text-text-default">
              {Math.round((completedCases / totalCases) * 100)}%
            </span>
          </div>
          <div className="w-full bg-background-default rounded-full h-2 overflow-hidden">
            <div
              className="h-full bg-background-accent rounded-full transition-all duration-300"
              style={{ width: `${(completedCases / totalCases) * 100}%` }}
            />
          </div>
        </div>
      )}

      {/* KPI row (shown when streaming or completed) */}
      {(isStreaming || runDetail) && (
        <>
          <div className="grid grid-cols-4 gap-3">
            <div className="rounded-lg bg-background-muted p-3 text-center">
              <div className={`text-lg font-bold ${kpiColor(overallAcc)}`}>
                {formatPercent(overallAcc)}
              </div>
              <div className="text-xs text-text-muted">Overall</div>
            </div>
            <div className="rounded-lg bg-background-muted p-3 text-center">
              <div className={`text-lg font-bold ${kpiColor(agentAcc)}`}>
                {formatPercent(agentAcc)}
              </div>
              <div className="text-xs text-text-muted">Agent</div>
            </div>
            <div className="rounded-lg bg-background-muted p-3 text-center">
              <div className={`text-lg font-bold ${kpiColor(modeAcc)}`}>
                {formatPercent(modeAcc)}
              </div>
              <div className="text-xs text-text-muted">Mode</div>
            </div>
            <div className="rounded-lg bg-background-muted p-3 text-center">
              <div className="text-lg font-bold text-text-default">
                {isStreaming ? `${completedCases}/${totalCases}` : totalCases}
              </div>
              <div className="text-xs text-text-muted">Cases</div>
              <div className="text-[10px] text-text-muted mt-0.5">
                {passCount} pass · {failureCount} fail
              </div>
            </div>
          </div>

          {/* Tab bar */}
          <div className="flex gap-1 border-b border-border-default">
            {tabs.map((t) => (
              <button
                key={t.key}
                type="button"
                onClick={() => setTab(t.key)}
                className={`px-4 py-2 text-xs font-medium border-b-2 transition-colors ${
                  tab === t.key
                    ? 'border-text-default text-text-default'
                    : 'border-transparent text-text-muted hover:text-text-default'
                }`}
              >
                {t.label}
                {t.count != null && (
                  <span
                    className={`ml-1.5 px-1.5 py-0.5 rounded-full text-[10px] ${
                      t.key === 'failures' && t.count > 0
                        ? 'bg-red-500/20 text-red-400'
                        : 'bg-background-muted text-text-muted'
                    }`}
                  >
                    {t.count}
                  </span>
                )}
              </button>
            ))}
          </div>

          {/* Tab content */}
          {tab === 'overview' && (
            <div className="space-y-4">
              {isStreaming ? (
                /* Live streaming results table */
                <div className="overflow-x-auto rounded-lg border border-border-default">
                  <table className="w-full text-xs">
                    <thead>
                      <tr className="bg-background-muted text-text-muted text-left">
                        <th className="px-3 py-2 w-8">#</th>
                        <th className="px-3 py-2 w-8">✓</th>
                        <th className="px-3 py-2">Input</th>
                        <th className="px-3 py-2">Expected</th>
                        <th className="px-3 py-2">Actual</th>
                        <th className="px-3 py-2 w-16">Conf.</th>
                      </tr>
                    </thead>
                    <tbody>
                      {streamProgress.results.map((r, idx) => (
                        <tr
                          key={`stream-${r.input.slice(0, 20)}-${r.expectedAgent}`}
                          className={`border-t border-border-default ${
                            !r.fullyCorrect ? 'bg-red-500/5' : ''
                          } ${idx === streamProgress.results.length - 1 ? 'animate-pulse' : ''}`}
                        >
                          <td className="px-3 py-2 text-text-muted">{idx + 1}</td>
                          <td className="px-3 py-2">
                            {r.fullyCorrect ? '✓' : r.agentCorrect ? '◐' : '✗'}
                          </td>
                          <td className="px-3 py-2 text-text-default max-w-[200px] truncate">
                            {r.input}
                          </td>
                          <td className="px-3 py-2 text-text-muted">
                            {r.expectedAgent} / {r.expectedMode}
                          </td>
                          <td className="px-3 py-2 text-text-default">
                            {r.actualAgent} / {r.actualMode}
                          </td>
                          <td className="px-3 py-2 font-mono">
                            {(r.confidence * 100).toFixed(0)}%
                          </td>
                        </tr>
                      ))}
                      {/* Pending rows */}
                      {Array.from({ length: Math.min(3, totalCases - completedCases) }).map(
                        (_, idx) => (
                          <tr
                            key={`pending-${completedCases + idx}`}
                            className="border-t border-border-default opacity-30"
                          >
                            <td className="px-3 py-2 text-text-muted">
                              {completedCases + idx + 1}
                            </td>
                            <td className="px-3 py-2">·</td>
                            <td className="px-3 py-2 text-text-muted italic" colSpan={4}>
                              pending...
                            </td>
                          </tr>
                        )
                      )}
                    </tbody>
                  </table>
                </div>
              ) : runDetail ? (
                <>
                  <div>
                    <h5 className="text-xs font-medium text-text-muted mb-2 uppercase tracking-wide">
                      Per-Agent Breakdown
                    </h5>
                    <PerAgentTable agents={runDetail.perAgent} />
                  </div>
                  <div className="flex gap-4 text-xs text-text-muted">
                    <span>Duration: {(runDetail.durationMs / 1000).toFixed(1)}s</span>
                    <span>Version: {runDetail.gooseVersion}</span>
                    {runDetail.versionTag && <span>Tag: {runDetail.versionTag}</span>}
                    <span>Started: {formatDate(runDetail.startedAt)}</span>
                  </div>
                </>
              ) : null}
            </div>
          )}

          {tab === 'matrix' &&
            (runDetail ? (
              <ApiConfusionMatrix matrix={runDetail.confusionMatrix} />
            ) : (
              <div className="text-center text-text-muted text-sm py-8">
                Confusion matrix available after evaluation completes.
              </div>
            ))}

          {tab === 'failures' &&
            (isStreaming ? (
              /* Live failures during streaming */
              streamProgress.results.filter((r) => !r.fullyCorrect).length === 0 ? (
                <div className="flex flex-col items-center justify-center h-32 text-text-muted">
                  <span className="text-2xl mb-2">✓</span>
                  <p className="text-sm">
                    No failures so far ({completedCases}/{totalCases} evaluated)
                  </p>
                </div>
              ) : (
                <div className="overflow-x-auto rounded-lg border border-border-default">
                  <table className="w-full text-xs">
                    <thead>
                      <tr className="bg-background-muted text-text-muted text-left">
                        <th className="px-3 py-2">Input</th>
                        <th className="px-3 py-2">Expected</th>
                        <th className="px-3 py-2">Actual</th>
                        <th className="px-3 py-2 w-16">Conf.</th>
                        <th className="px-3 py-2">Reasoning</th>
                      </tr>
                    </thead>
                    <tbody>
                      {streamProgress.results
                        .filter((r) => !r.fullyCorrect)
                        .map((r) => (
                          <tr
                            key={`sfail-${r.expectedAgent}-${r.actualAgent}-${r.input.slice(0, 30)}`}
                            className="border-t border-border-default bg-red-500/5"
                          >
                            <td className="px-3 py-2 text-text-default max-w-[200px] truncate">
                              {r.input}
                            </td>
                            <td className="px-3 py-2 text-text-muted">
                              {r.expectedAgent} / {r.expectedMode}
                            </td>
                            <td className="px-3 py-2 text-text-default">
                              {r.actualAgent} / {r.actualMode}
                            </td>
                            <td className="px-3 py-2 font-mono">
                              {(r.confidence * 100).toFixed(0)}%
                            </td>
                            <td className="px-3 py-2 text-text-muted max-w-[200px] truncate">
                              {r.reasoning || '—'}
                            </td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>
              )
            ) : failureCount === 0 ? (
              <div className="flex flex-col items-center justify-center h-32 text-text-muted">
                <span className="text-2xl mb-2">🎉</span>
                <p className="text-sm">All cases passed — no failures!</p>
              </div>
            ) : runDetail ? (
              <FailuresTable failures={runDetail.failures} />
            ) : null)}
        </>
      )}
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
    yaml: string
  ): Array<{
    input: string;
    expectedAgent: string;
    expectedMode: string;
    tags: string[];
  }> => {
    try {
      // Regex-based extraction for YAML (no YAML parser in browser)
      const entries: Array<{
        input: string;
        expectedAgent: string;
        expectedMode: string;
        tags: string[];
      }> = [];
      const blocks = yaml.split(/(?=- input:)/);
      for (const block of blocks) {
        const inputMatch = block.match(/- input:\s*"?([^"\n]+)"?/);
        const agentMatch = block.match(/expected_agent:\s*"?([^"\n]+)"?/);
        const modeMatch = block.match(/expected_mode:\s*"?([^"\n]+)"?/);
        const tagsMatch = block.match(/tags:\s*\[([^\]]*)\]/);
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
                    }))
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
        <label className="block">
          <span className="block text-xs text-text-muted mb-1">Name</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g., Core Routing Tests"
            className="w-full px-3 py-2 rounded-lg border border-border-default bg-background-default text-text-default text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </label>
        <label className="block">
          <span className="block text-xs text-text-muted mb-1">Description</span>
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Optional description"
            className="w-full px-3 py-2 rounded-lg border border-border-default bg-background-default text-text-default text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </label>
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
  const [runs, setRuns] = useState<EvalRunSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [editDataset, setEditDataset] = useState<EvalDataset | null>(null);
  const [runningDataset, setRunningDataset] = useState<string | null>(null);
  const [activeRunDetail, setActiveRunDetail] = useState<EvalRunDetail | null>(null);
  const [activeRunDatasetId, setActiveRunDatasetId] = useState<string | null>(null);
  const [view, setView] = useState<'datasets' | 'history'>('datasets');

  const fetchAll = useCallback(async () => {
    setLoading(true);
    try {
      const [dsResp, runsResp] = await Promise.all([listEvalDatasets(), listEvalRuns()]);
      setDatasets((dsResp.data as EvalDatasetSummary[]) ?? []);
      setRuns((runsResp.data as EvalRunSummary[]) ?? []);
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

  // ── Streaming eval state ──
  const [streamProgress, setStreamProgress] = useState<{
    index: number;
    total: number;
    results: Array<{
      input: string;
      expectedAgent: string;
      expectedMode: string;
      actualAgent: string;
      actualMode: string;
      confidence: number;
      agentCorrect: boolean;
      modeCorrect: boolean;
      fullyCorrect: boolean;
      reasoning: string;
    }>;
    metrics: { overallAccuracy: number; agentAccuracy: number; modeAccuracy: number };
  } | null>(null);

  const handleRunEval = async (datasetId: string) => {
    setRunningDataset(datasetId);
    setActiveRunDatasetId(datasetId);
    setActiveRunDetail(null);
    setStreamProgress(null);
    setError(null);

    try {
      const baseUrl = client.getConfig().baseUrl || '';
      // Build headers with auth secret (same pattern as RoutingInspector)
      const headers: Record<string, string> = { 'Content-Type': 'application/json' };
      const rawHeaders = client.getConfig().headers;
      if (rawHeaders) {
        const h = rawHeaders as Record<string, string>;
        const secretKey =
          typeof h.get === 'function'
            ? (h as unknown as globalThis.Headers).get('X-Secret-Key')
            : h['X-Secret-Key'];
        if (secretKey) {
          headers['X-Secret-Key'] = secretKey;
        }
      }

      const resp = await fetch(`${baseUrl}/analytics/eval/run/stream`, {
        method: 'POST',
        headers,
        body: JSON.stringify({ dataset_id: datasetId }),
      });

      if (!resp.ok) {
        throw new Error(`Eval failed: ${resp.status} ${resp.statusText}`);
      }

      const reader = resp.body?.getReader();
      if (!reader) throw new Error('No response stream');

      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.startsWith('data: ')) continue;
          const payload = line.slice(6).trim();
          if (!payload) continue;

          try {
            const evt = JSON.parse(payload);

            if (evt.type === 'progress') {
              const d = evt.data;
              setStreamProgress((prev) => {
                const results = [...(prev?.results || [])];
                results.push({
                  input: d.result.input,
                  expectedAgent: d.result.expected_agent,
                  expectedMode: d.result.expected_mode,
                  actualAgent: d.result.actual_agent,
                  actualMode: d.result.actual_mode,
                  confidence: d.result.confidence,
                  agentCorrect: d.result.agent_correct,
                  modeCorrect: d.result.mode_correct,
                  fullyCorrect: d.result.fully_correct,
                  reasoning: d.result.reasoning || '',
                });
                return {
                  index: d.index + 1,
                  total: d.total,
                  results,
                  metrics: {
                    overallAccuracy: d.running_metrics.overall_accuracy,
                    agentAccuracy: d.running_metrics.agent_accuracy,
                    modeAccuracy: d.running_metrics.mode_accuracy,
                  },
                };
              });
            } else if (evt.type === 'done') {
              // Stream finished — use the persisted detail from the event
              if (evt.detail) {
                setActiveRunDetail(evt.detail as EvalRunDetail);
              }
              setStreamProgress(null);
              await fetchAll();
            } else if (evt.type === 'error') {
              setError(evt.data?.message || 'Eval streaming error');
            }
          } catch {
            // skip malformed SSE lines
          }
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Eval run failed');
      setActiveRunDatasetId(null);
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

  if (activeRunDetail || activeRunDatasetId) {
    return (
      <RunDetailPanel
        runDetail={activeRunDetail}
        streamProgress={streamProgress}
        isRunning={runningDataset !== null}
        onClose={() => {
          setActiveRunDetail(null);
          setActiveRunDatasetId(null);
        }}
        onRerun={activeRunDatasetId ? () => handleRunEval(activeRunDatasetId) : undefined}
        onEdit={activeRunDatasetId ? () => handleEdit(activeRunDatasetId) : undefined}
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
                  <AccuracyBadge value={run.overallAccuracy} />
                  <span className="text-xs text-text-muted">{run.totalCases} cases</span>
                </div>
                <div className="flex gap-4 mt-1 text-xs text-text-muted">
                  <span>Agent: {formatPercent(run.agentAccuracy)}</span>
                  <span>Mode: {formatPercent(run.modeAccuracy)}</span>
                  <span>{formatDate(run.startedAt)}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
