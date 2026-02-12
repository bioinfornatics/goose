import { useEffect, useState, useCallback } from 'react';
import {
  Bot,
  Plus,
  Trash2,
  RefreshCw,
  ChevronDown,
  ChevronRight,
  Code,
  Plug,
  Cpu,
  Wrench,
  Puzzle,
} from 'lucide-react';
import {
  listAgents,
  connectAgent,
  disconnectAgent,
  listBuiltinAgents,
} from '../../api/sdk.gen';
import type { BuiltinAgentMode } from '../../api/types.gen';

// Unified agent type — both builtin and external
interface AgentCard {
  id: string;
  name: string;
  description: string;
  status: 'active' | 'connected' | 'disconnected';
  kind: 'builtin' | 'external';
  modes: BuiltinAgentMode[];
  defaultMode?: string;
}

export default function AgentsView() {
  const [agents, setAgents] = useState<AgentCard[]>([]);
  const [expandedAgent, setExpandedAgent] = useState<string | null>(null);
  const [selectedMode, setSelectedMode] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Connect form
  const [showConnect, setShowConnect] = useState(false);
  const [connectName, setConnectName] = useState('');

  const fetchAgents = useCallback(async () => {
    setLoading(true);
    const allAgents: AgentCard[] = [];

    // Fetch builtin agents
    try {
      const resp = await listBuiltinAgents();
      if (resp.data?.agents) {
        for (const agent of resp.data.agents) {
          allAgents.push({
            id: agent.name.toLowerCase().replace(/\s+/g, '-'),
            name: agent.name,
            description: agent.description,
            status: 'active',
            kind: 'builtin',
            modes: agent.modes,
            defaultMode: agent.default_mode,
          });
        }
      }
    } catch (e) {
      console.warn('Failed to fetch builtin agents:', e);
    }

    // Fetch external agents
    try {
      const resp = await listAgents();
      if (resp.data?.agents) {
        for (const agentId of resp.data.agents) {
          allAgents.push({
            id: agentId,
            name: agentId,
            description: 'External ACP agent',
            status: 'connected',
            kind: 'external',
            modes: [],
          });
        }
      }
    } catch (e) {
      console.warn('Failed to fetch external agents:', e);
    }

    setAgents(allAgents);
    setLoading(false);
  }, []);

  useEffect(() => { fetchAgents(); }, [fetchAgents]);

  const handleConnect = async () => {
    if (!connectName.trim()) return;
    setError(null);
    try {
      await connectAgent({ body: { name: connectName.trim() } });
      setConnectName('');
      setShowConnect(false);
      fetchAgents();
    } catch (e) {
      setError(`Connect failed: ${e}`);
    }
  };

  const handleDisconnect = async (id: string) => {
    try {
      await disconnectAgent({ path: { agent_id: id } });
      fetchAgents();
    } catch (e) {
      setError(`Disconnect failed: ${e}`);
    }
  };

  const getAgentIcon = (agent: AgentCard) => {
    if (agent.name === 'Goose Agent') return <Bot className="w-6 h-6" />;
    if (agent.name === 'Coding Agent') return <Code className="w-6 h-6" />;
    if (agent.kind === 'external') return <Plug className="w-6 h-6" />;
    return <Cpu className="w-6 h-6" />;
  };

  const getStatusStyle = (status: string) => {
    switch (status) {
      case 'active': return { color: 'text-emerald-500', bg: 'bg-emerald-500', label: 'Active' };
      case 'connected': return { color: 'text-blue-500', bg: 'bg-blue-500', label: 'Connected' };
      default: return { color: 'text-gray-400', bg: 'bg-gray-400', label: 'Offline' };
    }
  };

  const getKindBadge = (kind: string) => {
    if (kind === 'builtin') return { bg: 'bg-violet-100 dark:bg-violet-900/30', text: 'text-violet-700 dark:text-violet-300', label: 'Built-in' };
    return { bg: 'bg-sky-100 dark:bg-sky-900/30', text: 'text-sky-700 dark:text-sky-300', label: 'External' };
  };

  const toolGroupColor = (group: string): string => {
    const map: Record<string, string> = {
      developer: 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300',
      command: 'bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300',
      edit: 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-300',
      read: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300',
      memory: 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300',
      fetch: 'bg-orange-100 text-orange-700 dark:bg-orange-900/40 dark:text-orange-300',
      browser: 'bg-pink-100 text-pink-700 dark:bg-pink-900/40 dark:text-pink-300',
      mcp: 'bg-indigo-100 text-indigo-700 dark:bg-indigo-900/40 dark:text-indigo-300',
    };
    return map[group] || 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300';
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-5xl mx-auto p-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-2xl font-bold flex items-center gap-2.5">
              <Bot className="w-7 h-7 text-blue-500" />
              Agents
            </h1>
            <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">
              {agents.length} agent{agents.length !== 1 ? 's' : ''} available
              {' · '}{agents.filter(a => a.modes.length > 0).reduce((sum, a) => sum + a.modes.length, 0)} modes
            </p>
          </div>
          <div className="flex gap-2">
            <button
              onClick={() => setShowConnect(!showConnect)}
              className="flex items-center gap-1.5 px-3 py-2 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors"
            >
              <Plus className="w-4 h-4" />
              Connect Agent
            </button>
            <button
              onClick={fetchAgents}
              className="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
              title="Refresh"
            >
              <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            </button>
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="mb-6 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-red-700 dark:text-red-300 text-sm flex justify-between">
            <span>{error}</span>
            <button onClick={() => setError(null)} className="underline text-xs">dismiss</button>
          </div>
        )}

        {/* Connect Form */}
        {showConnect && (
          <div className="mb-6 p-4 border-2 border-dashed border-blue-300 dark:border-blue-700 rounded-xl bg-blue-50/50 dark:bg-blue-900/10">
            <p className="text-sm font-medium mb-2">Connect an external agent</p>
            <div className="flex gap-2">
              <input
                value={connectName}
                onChange={(e) => setConnectName(e.target.value)}
                placeholder="Agent name from registry..."
                className="flex-1 px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:outline-none focus:ring-2 focus:ring-blue-500"
                onKeyDown={(e) => e.key === 'Enter' && handleConnect()}
                autoFocus
              />
              <button onClick={handleConnect} className="px-4 py-2 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600">
                Connect
              </button>
              <button onClick={() => setShowConnect(false)} className="px-3 py-2 text-sm text-gray-500 hover:text-gray-700">
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Agent Cards Grid */}
        {loading ? (
          <div className="text-center py-16 text-gray-400">
            <RefreshCw className="w-8 h-8 mx-auto mb-3 animate-spin" />
            <p>Loading agents...</p>
          </div>
        ) : agents.length === 0 ? (
          <div className="text-center py-16 text-gray-400">
            <Bot className="w-12 h-12 mx-auto mb-3 opacity-30" />
            <p className="text-lg font-medium">No agents available</p>
            <p className="text-sm mt-1">Connect an external agent to get started</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {agents.map((agent) => {
              const status = getStatusStyle(agent.status);
              const kind = getKindBadge(agent.kind);
              const isExpanded = expandedAgent === agent.id;

              return (
                <div
                  key={agent.id}
                  className={`rounded-xl border transition-all duration-200 ${
                    isExpanded
                      ? 'border-blue-300 dark:border-blue-700 shadow-lg col-span-1 md:col-span-2'
                      : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600 hover:shadow-md'
                  }`}
                >
                  {/* Card Header */}
                  <div
                    className="p-4 cursor-pointer select-none"
                    onClick={() => setExpandedAgent(isExpanded ? null : agent.id)}
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex items-start gap-3">
                        <div className="mt-0.5 text-gray-600 dark:text-gray-300">
                          {getAgentIcon(agent)}
                        </div>
                        <div>
                          <div className="flex items-center gap-2">
                            <h3 className="font-semibold">{agent.name}</h3>
                            <span className={`text-[10px] px-1.5 py-0.5 rounded-full font-medium ${kind.bg} ${kind.text}`}>
                              {kind.label}
                            </span>
                          </div>
                          <p className="text-sm text-gray-500 dark:text-gray-400 mt-0.5">
                            {agent.description}
                          </p>
                        </div>
                      </div>
                      <div className="flex items-center gap-3 shrink-0">
                        <div className="flex items-center gap-1.5">
                          <span className={`w-2 h-2 rounded-full ${status.bg}`} />
                          <span className={`text-xs ${status.color}`}>{status.label}</span>
                        </div>
                        {agent.modes.length > 0 && (
                          <span className="text-xs text-gray-400 bg-gray-100 dark:bg-gray-800 px-2 py-0.5 rounded-full">
                            {agent.modes.length} modes
                          </span>
                        )}
                        {isExpanded ? (
                          <ChevronDown className="w-4 h-4 text-gray-400" />
                        ) : (
                          <ChevronRight className="w-4 h-4 text-gray-400" />
                        )}
                      </div>
                    </div>

                    {/* External agent actions */}
                    {agent.kind === 'external' && (
                      <div className="mt-3 flex gap-2">
                        <button
                          onClick={(e) => { e.stopPropagation(); handleDisconnect(agent.id); }}
                          className="flex items-center gap-1 px-2.5 py-1 text-xs text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-md transition-colors"
                        >
                          <Trash2 className="w-3 h-3" />
                          Disconnect
                        </button>
                      </div>
                    )}
                  </div>

                  {/* Expanded: Modes Grid */}
                  {isExpanded && agent.modes.length > 0 && (
                    <div className="border-t border-gray-200 dark:border-gray-700 p-4 bg-gray-50/50 dark:bg-gray-800/20">
                      <div className="flex items-center gap-2 mb-3">
                        <Wrench className="w-4 h-4 text-gray-400" />
                        <span className="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                          Available Modes
                        </span>
                      </div>
                      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                        {agent.modes.map((mode) => {
                          const isSelected = selectedMode === `${agent.id}:${mode.slug}`;
                          const isDefault = mode.slug === agent.defaultMode;
                          return (
                            <div
                              key={mode.slug}
                              onClick={() => setSelectedMode(isSelected ? null : `${agent.id}:${mode.slug}`)}
                              className={`p-3 rounded-lg border cursor-pointer transition-all ${
                                isSelected
                                  ? 'border-blue-400 dark:border-blue-600 bg-blue-50 dark:bg-blue-900/20 ring-1 ring-blue-400/30'
                                  : isDefault
                                  ? 'border-emerald-200 dark:border-emerald-800 bg-emerald-50/30 dark:bg-emerald-900/10'
                                  : 'border-gray-200 dark:border-gray-600 hover:border-gray-300 dark:hover:border-gray-500'
                              }`}
                            >
                              <div className="flex items-center justify-between mb-1.5">
                                <span className="font-medium text-sm">{mode.name}</span>
                                {isDefault && (
                                  <span className="text-[9px] bg-emerald-100 dark:bg-emerald-800/50 text-emerald-700 dark:text-emerald-300 px-1.5 py-0.5 rounded-full font-semibold">
                                    DEFAULT
                                  </span>
                                )}
                              </div>
                              <p className="text-xs text-gray-500 dark:text-gray-400 line-clamp-2 mb-2">
                                {mode.description}
                              </p>

                              {/* Tool groups */}
                              {mode.tool_groups.length > 0 && (
                                <div className="flex flex-wrap gap-1 mb-1.5">
                                  {mode.tool_groups.map((tg) => (
                                    <span key={tg} className={`text-[10px] px-1.5 py-0.5 rounded-full font-medium ${toolGroupColor(tg)}`}>
                                      <Wrench className="w-2.5 h-2.5 inline mr-0.5" />
                                      {tg}
                                    </span>
                                  ))}
                                </div>
                              )}

                              {/* Recommended extensions */}
                              {mode.recommended_extensions.length > 0 && (
                                <div className="flex flex-wrap gap-1">
                                  {mode.recommended_extensions.map((ext) => (
                                    <span key={ext} className="text-[10px] px-1.5 py-0.5 rounded-full bg-gray-100 dark:bg-gray-700/50 text-gray-500 dark:text-gray-400 border border-gray-200 dark:border-gray-600">
                                      <Puzzle className="w-2.5 h-2.5 inline mr-0.5" />
                                      {ext}
                                    </span>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
