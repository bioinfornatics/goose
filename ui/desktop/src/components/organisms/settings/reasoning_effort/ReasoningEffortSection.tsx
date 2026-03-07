import { useEffect, useState } from 'react';
import {
  agentCatalog,
  getReasoningEffort,
  getReasoningEffortOverrides,
  setReasoningEffort,
  setReasoningEffortOverrides,
} from '@/api';
import type { CatalogAgent } from '@/api/types.gen';
import { useModelAndProvider } from '@/contexts/ModelAndProviderContext';
import {
  ReasoningEffortSelectionItem,
  reasoningEffortOptions,
} from './ReasoningEffortSelectionItem';
import { supportsReasoningEffort } from './reasoningEffortUtils';

type OverrideMap = Record<string, string>; // "agent_id/mode_slug" → "low"|"medium"|"high"

export const ReasoningEffortSection = () => {
  const { currentModel, currentProvider } = useModelAndProvider();
  const supported = supportsReasoningEffort(currentModel, currentProvider);
  const [globalLevel, setGlobalLevel] = useState('medium');
  const [overrides, setOverrides] = useState<OverrideMap>({});
  const [agents, setAgents] = useState<CatalogAgent[]>([]);
  const [loading, setLoading] = useState(true);
  const [showOverrides, setShowOverrides] = useState(false);

  useEffect(() => {
    const fetchAll = async () => {
      try {
        const [effortRes, overridesRes, catalogRes] = await Promise.all([
          getReasoningEffort(),
          getReasoningEffortOverrides(),
          agentCatalog(),
        ]);

        const level = effortRes.data?.level;
        if (level && ['low', 'medium', 'high'].includes(level)) {
          setGlobalLevel(level);
        }

        if (overridesRes.data?.overrides) {
          const map: OverrideMap = {};
          for (const o of overridesRes.data.overrides) {
            map[o.key] = o.level;
          }
          setOverrides(map);
          if (Object.keys(map).length > 0) {
            setShowOverrides(true);
          }
        }

        if (catalogRes.data?.agents) {
          const builtin = catalogRes.data.agents.filter(
            (a) => a.kind === 'builtin' && a.status === 'active'
          );
          setAgents(builtin);
        }
      } catch (error) {
        console.error('Error fetching reasoning effort config:', error);
      } finally {
        setLoading(false);
      }
    };
    fetchAll();
  }, []);

  const handleGlobalChange = async (newLevel: string) => {
    const previous = globalLevel;
    setGlobalLevel(newLevel);
    try {
      await setReasoningEffort({ body: { level: newLevel } });
    } catch (error) {
      console.error('Error setting global reasoning effort:', error);
      setGlobalLevel(previous);
    }
  };

  const handleOverrideChange = async (
    agentId: string,
    modeSlug: string,
    newLevel: string | null
  ) => {
    const key = `${agentId}/${modeSlug}`;
    const previous = { ...overrides };

    const next = { ...overrides };
    if (newLevel === null || newLevel === globalLevel) {
      delete next[key];
    } else {
      next[key] = newLevel;
    }
    setOverrides(next);

    try {
      const overrideList = Object.entries(next).map(([k, level]) => ({
        key: k,
        level,
      }));
      await setReasoningEffortOverrides({
        body: { overrides: overrideList },
      });
    } catch (error) {
      console.error('Error setting reasoning effort overrides:', error);
      setOverrides(previous);
    }
  };

  if (loading) {
    return (
      <div className="space-y-1">
        {reasoningEffortOptions.map((option) => (
          <div key={option.key} className="h-12 bg-background-muted rounded-lg animate-pulse" />
        ))}
      </div>
    );
  }

  if (!supported) {
    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2 text-text-muted">
          <svg
            className="w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <title>Info</title>
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span className="text-sm">Not available for the current model</span>
        </div>
        <p className="text-xs text-text-muted pl-6">
          Reasoning effort is supported by OpenAI o-series, GPT-5, Anthropic Claude, and Gemini
          thinking models. Switch to a supported model to configure reasoning effort.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Global default */}
      <div>
        <p className="text-sm text-text-muted mb-2">Global default</p>
        <div className="space-y-1">
          {reasoningEffortOptions.map((option) => (
            <ReasoningEffortSelectionItem
              key={option.key}
              option={option}
              currentLevel={globalLevel}
              showDescription={true}
              handleLevelChange={handleGlobalChange}
            />
          ))}
        </div>
      </div>

      {/* Per-agent/mode overrides */}
      {agents.length > 0 && (
        <div>
          <button
            type="button"
            onClick={() => setShowOverrides(!showOverrides)}
            className="flex items-center gap-2 text-sm text-text-muted hover:text-text-default transition-colors"
          >
            <svg
              className={`w-3 h-3 transition-transform ${showOverrides ? 'rotate-90' : ''}`}
              fill="currentColor"
              viewBox="0 0 20 20"
            >
              <title>Toggle</title>
              <path
                fillRule="evenodd"
                d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z"
                clipRule="evenodd"
              />
            </svg>
            Per-agent overrides
            {Object.keys(overrides).length > 0 && (
              <span className="text-xs bg-accent/20 text-accent px-1.5 py-0.5 rounded-full">
                {Object.keys(overrides).length}
              </span>
            )}
          </button>

          {showOverrides && (
            <div className="mt-3 space-y-3">
              {agents.map((agent) => (
                <AgentOverrideGroup
                  key={agent.id}
                  agent={agent}
                  overrides={overrides}
                  globalLevel={globalLevel}
                  onOverrideChange={handleOverrideChange}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

const AgentOverrideGroup = ({
  agent,
  overrides,
  globalLevel,
  onOverrideChange,
}: {
  agent: CatalogAgent;
  overrides: OverrideMap;
  globalLevel: string;
  onOverrideChange: (agentId: string, modeSlug: string, level: string | null) => void;
}) => {
  return (
    <div className="border border-border-subtle rounded-lg p-3">
      <p className="text-sm font-medium mb-2">{agent.name}</p>
      <div className="space-y-1.5">
        {agent.modes.map((mode) => {
          const key = `${agent.id}/${mode.slug}`;
          const currentValue = overrides[key] || null;

          return (
            <ModeOverrideRow
              key={mode.slug}
              modeName={mode.name}
              modeSlug={mode.slug}
              currentValue={currentValue}
              globalLevel={globalLevel}
              onChange={(level) => onOverrideChange(agent.id, mode.slug, level)}
            />
          );
        })}
      </div>
    </div>
  );
};

const ModeOverrideRow = ({
  modeName,
  modeSlug,
  currentValue,
  globalLevel,
  onChange,
}: {
  modeName: string;
  modeSlug: string;
  currentValue: string | null;
  globalLevel: string;
  onChange: (level: string | null) => void;
}) => {
  const options = [
    { value: '', label: `Default (${globalLevel})` },
    { value: 'low', label: 'Low' },
    { value: 'medium', label: 'Medium' },
    { value: 'high', label: 'High' },
  ];

  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-sm text-text-muted truncate" title={modeSlug}>
        {modeName}
      </span>
      <select
        value={currentValue || ''}
        onChange={(e) => onChange(e.target.value || null)}
        className="text-sm bg-background-default border border-border-subtle rounded px-2 py-1 min-w-[120px] text-text-default"
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
};
