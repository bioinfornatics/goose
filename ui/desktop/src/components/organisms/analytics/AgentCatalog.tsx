import { useCallback, useEffect, useState } from 'react';
import { client } from '@/api/client.gen';
import { Badge } from '@/components/atoms/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/atoms/tooltip';

// ── Types ──────────────────────────────────────────────────────

interface CatalogMode {
  slug: string;
  name: string;
  description: string;
  whenToUse: string;
  toolGroups: string[];
}

interface CatalogAgent {
  name: string;
  description: string;
  enabled: boolean;
  modes: CatalogMode[];
}

interface CatalogResponse {
  agents: CatalogAgent[];
}

// ── Atoms: ToolGroupBadge ──────────────────────────────────────

/** Semantic color + icon mapping for tool groups */
const toolGroupMeta: Record<string, { color: string; icon: string; label: string }> = {
  read: { color: 'bg-sky-500/15 text-sky-400 border-sky-500/30', icon: '👁', label: 'Read' },
  edit: { color: 'bg-amber-500/15 text-amber-400 border-amber-500/30', icon: '✏️', label: 'Edit' },
  command: {
    color: 'bg-purple-500/15 text-purple-400 border-purple-500/30',
    icon: '⌨',
    label: 'Command',
  },
  mcp: {
    color: 'bg-green-500/15 text-green-400 border-green-500/30',
    icon: '🔌',
    label: 'MCP',
  },
  developer: {
    color: 'bg-indigo-500/15 text-indigo-400 border-indigo-500/30',
    icon: '🛠',
    label: 'Developer',
  },
  apps: {
    color: 'bg-pink-500/15 text-pink-400 border-pink-500/30',
    icon: '📱',
    label: 'Apps',
  },
  code_execution: {
    color: 'bg-orange-500/15 text-orange-400 border-orange-500/30',
    icon: '▶',
    label: 'Code Exec',
  },
  diagnostics: {
    color: 'bg-teal-500/15 text-teal-400 border-teal-500/30',
    icon: '🔍',
    label: 'Diagnostics',
  },
  fetch: {
    color: 'bg-cyan-500/15 text-cyan-400 border-cyan-500/30',
    icon: '🌐',
    label: 'Fetch',
  },
  memory: {
    color: 'bg-violet-500/15 text-violet-400 border-violet-500/30',
    icon: '🧠',
    label: 'Memory',
  },
  genui: {
    color: 'bg-rose-500/15 text-rose-400 border-rose-500/30',
    icon: '🎨',
    label: 'GenUI',
  },
};

const defaultToolMeta = {
  color: 'bg-gray-500/15 text-gray-400 border-gray-500/30',
  icon: '⚙',
  label: '',
};

function ToolGroupBadge({ group }: { group: string }) {
  const raw = group.replace(/\s*\(restricted\)/, '');
  const isRestricted = group.includes('(restricted)');
  const meta = toolGroupMeta[raw] ?? { ...defaultToolMeta, label: raw };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium cursor-default transition-colors hover:brightness-110 ${meta.color}`}
        >
          <span className="text-[11px]">{meta.icon}</span>
          {meta.label}
          {isRestricted && (
            <span className="text-[9px] opacity-70" title="Restricted access">
              ⚠
            </span>
          )}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-[200px] text-xs">
        <strong>{meta.label || raw}</strong>
        {isRestricted ? ' — restricted file access' : ' — full access'}
      </TooltipContent>
    </Tooltip>
  );
}

// ── Atoms: ModePhaseBadge ──────────────────────────────────────

const modePhaseColors: Record<string, string> = {
  ask: 'bg-sky-500/20 text-sky-300',
  plan: 'bg-violet-500/20 text-violet-300',
  write: 'bg-emerald-500/20 text-emerald-300',
  review: 'bg-amber-500/20 text-amber-300',
  debug: 'bg-red-500/20 text-red-300',
};

function ModePhaseBadge({ slug }: { slug: string }) {
  const color = modePhaseColors[slug] ?? 'bg-gray-500/20 text-gray-400';
  return (
    <span
      className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider ${color}`}
    >
      {slug}
    </span>
  );
}

// ── Molecules: ModeCard ────────────────────────────────────────

function ModeCard({ mode }: { mode: CatalogMode }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="group px-4 py-3 hover:bg-background-muted/50 transition-colors">
      <div className="flex items-start gap-3">
        {/* Phase badge */}
        <div className="pt-0.5">
          <ModePhaseBadge slug={mode.slug} />
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          {/* Mode name + tool badges */}
          <div className="flex items-center gap-2 flex-wrap">
            {mode.name && mode.name !== mode.slug && (
              <span className="text-sm font-medium text-text-default">{mode.name}</span>
            )}
            {mode.toolGroups.length > 0 && (
              <div className="flex items-center gap-1 flex-wrap">
                {mode.toolGroups.map((tg) => (
                  <ToolGroupBadge key={tg} group={tg} />
                ))}
              </div>
            )}
          </div>

          {/* Description */}
          {mode.description && (
            <p className="text-sm text-text-muted mt-1 leading-snug">{mode.description}</p>
          )}

          {/* When to use — collapsible */}
          {mode.whenToUse && (
            <button
              type="button"
              onClick={() => setExpanded(!expanded)}
              className="text-[11px] text-text-muted mt-1.5 flex items-center gap-1 hover:text-text-default transition-colors cursor-pointer"
            >
              <span
                className="transform transition-transform"
                style={{
                  display: 'inline-block',
                  transform: expanded ? 'rotate(90deg)' : 'rotate(0deg)',
                }}
              >
                ›
              </span>
              When to use
            </button>
          )}
          {expanded && mode.whenToUse && (
            <p className="text-xs text-text-muted mt-1 pl-3 border-l-2 border-border-default italic leading-relaxed">
              {mode.whenToUse}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Organisms: AgentCard ───────────────────────────────────────

function AgentCard({
  agent,
  onToggle,
  toggling,
}: {
  agent: CatalogAgent;
  onToggle: (name: string) => void;
  toggling: boolean;
}) {
  // Collect all unique tool groups across modes
  const allTools = [...new Set(agent.modes.flatMap((m) => m.toolGroups))];

  return (
    <div
      className={`rounded-lg border overflow-hidden transition-all ${
        agent.enabled
          ? 'border-border-default bg-background-muted shadow-sm'
          : 'border-border-default/50 bg-background-muted/50 opacity-70'
      }`}
    >
      {/* ── Agent Header ── */}
      <div className="px-4 py-3 border-b border-border-default">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <h3 className="text-base font-semibold text-text-default">{agent.name}</h3>
            <Badge variant={agent.enabled ? 'default' : 'muted'} size="sm">
              {agent.modes.length} mode{agent.modes.length !== 1 ? 's' : ''}
            </Badge>
          </div>
          <button
            type="button"
            onClick={() => onToggle(agent.name)}
            disabled={toggling}
            className={`rounded-full px-2.5 py-1 text-xs font-medium transition-all cursor-pointer ${
              toggling
                ? 'bg-background-muted/40 text-text-muted border border-border-default opacity-50'
                : agent.enabled
                  ? 'bg-background-success-muted text-text-success border border-border-default hover:bg-background-danger-muted hover:text-text-danger'
                  : 'bg-background-muted/40 text-text-muted border border-border-default hover:bg-background-success-muted hover:text-text-success'
            }`}
          >
            {toggling ? 'toggling…' : agent.enabled ? 'enabled' : 'disabled'}
          </button>
        </div>

        {agent.description && <p className="text-sm text-text-muted mt-1">{agent.description}</p>}

        {/* ── Capabilities summary (all tool groups across modes) ── */}
        {allTools.length > 0 && (
          <div className="flex items-center gap-1.5 mt-2 flex-wrap">
            <span className="text-[10px] text-text-muted uppercase tracking-wider font-medium">
              Capabilities:
            </span>
            {allTools.map((tg) => (
              <ToolGroupBadge key={tg} group={tg} />
            ))}
          </div>
        )}
      </div>

      {/* ── Modes List ── */}
      <div className="divide-y divide-border-default/50">
        {agent.modes.map((mode) => (
          <ModeCard key={mode.slug} mode={mode} />
        ))}
      </div>
    </div>
  );
}

// ── Main Component ─────────────────────────────────────────────

export default function AgentCatalog() {
  const [catalog, setCatalog] = useState<CatalogAgent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [toggling, setToggling] = useState<string | null>(null);

  const fetchCatalog = useCallback(async () => {
    try {
      setLoading(true);
      const cfg = client.getConfig();
      const rawHeaders = cfg.headers;
      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
      };
      if (rawHeaders instanceof Headers) {
        rawHeaders.forEach((v, k) => {
          headers[k] = v;
        });
      } else if (rawHeaders && typeof rawHeaders === 'object') {
        Object.assign(headers, rawHeaders as Record<string, string>);
      }
      const resp = await fetch(`${cfg.baseUrl}/analytics/routing/catalog`, { headers });
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const data = (await resp.json()) as CatalogResponse;
      setCatalog(data.agents);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load catalog');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchCatalog();
  }, [fetchCatalog]);

  const handleToggle = useCallback(
    async (agentName: string) => {
      try {
        setToggling(agentName);
        const cfg = client.getConfig();
        const rawHeaders = cfg.headers;
        const headers: Record<string, string> = {
          'Content-Type': 'application/json',
        };
        if (rawHeaders instanceof Headers) {
          rawHeaders.forEach((v, k) => {
            headers[k] = v;
          });
        } else if (rawHeaders && typeof rawHeaders === 'object') {
          Object.assign(headers, rawHeaders as Record<string, string>);
        }
        const slug = agentName.toLowerCase().replace(/\s+/g, '-');
        const agent = catalog.find((a) => a.name === agentName);
        const action = agent?.enabled ? 'disable' : 'enable';
        await fetch(`${cfg.baseUrl}/agents/builtin/${slug}/toggle`, {
          method: 'POST',
          headers,
          body: JSON.stringify({ action }),
        });
        await fetchCatalog();
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Failed to toggle agent');
      } finally {
        setToggling(null);
      }
    },
    [catalog, fetchCatalog]
  );

  // ── Loading state ──
  if (loading) {
    return (
      <div className="space-y-4">
        {[1, 2, 3].map((i) => (
          <div
            key={i}
            className="rounded-lg border border-border-default bg-background-muted animate-pulse"
          >
            <div className="px-4 py-3 border-b border-border-default space-y-2">
              <div className="h-5 w-40 bg-background-muted rounded" />
              <div className="h-3 w-64 bg-background-muted rounded" />
            </div>
            <div className="px-4 py-3 space-y-2">
              <div className="h-4 w-full bg-background-muted rounded" />
              <div className="h-4 w-3/4 bg-background-muted rounded" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <div className="space-y-3">
        <div className="rounded-md bg-background-danger-muted border border-border-default px-3 py-2 text-sm text-text-danger">
          {error}
        </div>
        <button
          type="button"
          onClick={fetchCatalog}
          className="rounded-md border border-border-default px-3 py-1.5 text-sm text-text-muted hover:bg-background-muted hover:text-text-default"
        >
          Retry
        </button>
      </div>
    );
  }

  if (catalog.length === 0) {
    return (
      <div className="text-center py-12 text-text-muted text-sm">No agents found in catalog.</div>
    );
  }

  return (
    <TooltipProvider delayDuration={200}>
      <div className="space-y-4">
        {/* ── Summary bar ── */}
        <div className="flex items-center gap-4 text-xs text-text-muted">
          <span>
            {catalog.filter((a) => a.enabled).length}/{catalog.length} agents enabled
          </span>
          <span>·</span>
          <span>{catalog.reduce((n, a) => n + a.modes.length, 0)} total modes</span>
          <span>·</span>
          <span>
            {[...new Set(catalog.flatMap((a) => a.modes.flatMap((m) => m.toolGroups)))].length} tool
            groups
          </span>
        </div>

        {/* ── Agent cards ── */}
        {catalog.map((agent) => (
          <AgentCard
            key={agent.name}
            agent={agent}
            onToggle={handleToggle}
            toggling={toggling === agent.name}
          />
        ))}
      </div>
    </TooltipProvider>
  );
}
