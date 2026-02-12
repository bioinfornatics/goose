import { useEffect, useState, useCallback } from 'react';
import {
  Bot,
  Plus,
  Trash2,
  Play,
  RefreshCw,
  ChevronDown,
  ChevronRight,
  Cpu,
  Code,
  Circle,
  Plug,
} from 'lucide-react';
import {
  listAgents,
  connectAgent,
  disconnectAgent,
  createSession,
  promptAgent,
  listBuiltinAgents,
} from '../../api/sdk.gen';
import type { BuiltinAgentInfo, BuiltinAgentMode } from '../../api/types.gen';

interface ConnectedAgent {
  id: string;
  sessionId?: string;
}

export default function AgentsView() {
  // Builtin agents
  const [builtinAgents, setBuiltinAgents] = useState<BuiltinAgentInfo[]>([]);
  const [expandedAgents, setExpandedAgents] = useState<Set<string>>(new Set(['Goose Agent', 'Coding Agent']));
  const [selectedMode, setSelectedMode] = useState<string | null>(null);

  // External agents
  const [externalAgents, setExternalAgents] = useState<ConnectedAgent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Connect form
  const [showConnect, setShowConnect] = useState(false);
  const [connectName, setConnectName] = useState('');

  // Prompt
  const [promptAgentId, setPromptAgentId] = useState<string | null>(null);
  const [promptText, setPromptText] = useState('');
  const [promptResponse, setPromptResponse] = useState<string | null>(null);
  const [prompting, setPrompting] = useState(false);

  const fetchBuiltinAgents = useCallback(async () => {
    try {
      const resp = await listBuiltinAgents();
      if (resp.data) {
        setBuiltinAgents(resp.data.agents);
      }
    } catch {
      // Builtin agents are always available, this shouldn't fail normally
    }
  }, []);

  const fetchExternalAgents = useCallback(async () => {
    setLoading(true);
    try {
      const resp = await listAgents();
      if (resp.data) {
        setExternalAgents(
          resp.data.agents.map((a: string) => ({ id: a }))
        );
      }
    } catch (e) {
      setError(`Failed to list agents: ${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchBuiltinAgents();
    fetchExternalAgents();
  }, [fetchBuiltinAgents, fetchExternalAgents]);

  const toggleAgent = (name: string) => {
    setExpandedAgents(prev => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const handleConnect = async () => {
    if (!connectName.trim()) return;
    setError(null);
    try {
      await connectAgent({ body: { name: connectName.trim() } });
      setConnectName('');
      setShowConnect(false);
      fetchExternalAgents();
    } catch (e) {
      setError(`Connect failed: ${e}`);
    }
  };

  const handleDisconnect = async (id: string) => {
    try {
      await disconnectAgent({ path: { agent_id: id } });
      fetchExternalAgents();
    } catch (e) {
      setError(`Disconnect failed: ${e}`);
    }
  };

  const handlePrompt = async (agentId: string) => {
    if (!promptText.trim()) return;
    setPrompting(true);
    setPromptResponse(null);
    try {
      // Create session first
      const sessResp = await createSession({ path: { agent_id: agentId }, body: {} });
      const sessionId = sessResp.data?.session_id;
      if (!sessionId) throw new Error('No session ID returned');

      const resp = await promptAgent({
        path: { agent_id: agentId },
        body: { session_id: sessionId, text: promptText },
      });
      setPromptResponse(resp.data?.text || '(empty response)');
    } catch (e) {
      setPromptResponse(`Error: ${e}`);
    } finally {
      setPrompting(false);
    }
  };

  const getAgentIcon = (name: string) => {
    if (name === 'Goose Agent') return <Bot className="w-5 h-5" />;
    if (name === 'Coding Agent') return <Code className="w-5 h-5" />;
    return <Cpu className="w-5 h-5" />;
  };

  const getStatusColor = (status: string) => {
    if (status === 'active') return 'text-green-500';
    if (status === 'degraded') return 'text-yellow-500';
    return 'text-gray-400';
  };

  const getModeToolBadge = (group: string) => {
    const colors: Record<string, string> = {
      developer: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200',
      command: 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200',
      edit: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
      read: 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-200',
      memory: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200',
      fetch: 'bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-200',
      browser: 'bg-pink-100 text-pink-800 dark:bg-pink-900 dark:text-pink-200',
      mcp: 'bg-indigo-100 text-indigo-800 dark:bg-indigo-900 dark:text-indigo-200',
    };
    return colors[group] || 'bg-gray-100 text-gray-700 dark:bg-gray-700 dark:text-gray-300';
  };

  return (
    <div className="h-full overflow-y-auto p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <Bot className="w-7 h-7 text-blue-500" />
          <h1 className="text-2xl font-bold">Agents</h1>
        </div>
        <button
          onClick={() => { fetchBuiltinAgents(); fetchExternalAgents(); }}
          className="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
          title="Refresh"
        >
          <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-red-700 dark:text-red-300 text-sm">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">dismiss</button>
        </div>
      )}

      {/* Builtin Agents Section */}
      <section className="mb-8">
        <h2 className="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-3">
          Builtin Agents
        </h2>
        <div className="space-y-3">
          {builtinAgents.map((agent) => (
            <div
              key={agent.name}
              className="border border-gray-200 dark:border-gray-700 rounded-xl overflow-hidden"
            >
              {/* Agent Header */}
              <button
                onClick={() => toggleAgent(agent.name)}
                className="w-full flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
              >
                <div className="flex items-center gap-3">
                  {getAgentIcon(agent.name)}
                  <div className="text-left">
                    <div className="font-semibold">{agent.name}</div>
                    <div className="text-sm text-gray-500 dark:text-gray-400">
                      {agent.description}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <div className="flex items-center gap-1.5">
                    <Circle className={`w-2.5 h-2.5 fill-current ${getStatusColor(agent.status)}`} />
                    <span className={`text-xs font-medium ${getStatusColor(agent.status)}`}>
                      {agent.status}
                    </span>
                  </div>
                  <span className="text-xs text-gray-400">
                    {agent.modes.length} modes
                  </span>
                  {expandedAgents.has(agent.name) ? (
                    <ChevronDown className="w-4 h-4 text-gray-400" />
                  ) : (
                    <ChevronRight className="w-4 h-4 text-gray-400" />
                  )}
                </div>
              </button>

              {/* Modes Grid */}
              {expandedAgents.has(agent.name) && (
                <div className="border-t border-gray-200 dark:border-gray-700 p-4 bg-gray-50/50 dark:bg-gray-800/30">
                  <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                    {agent.modes.map((mode: BuiltinAgentMode) => (
                      <button
                        key={mode.slug}
                        onClick={() => setSelectedMode(selectedMode === mode.slug ? null : mode.slug)}
                        className={`text-left p-3 rounded-lg border transition-all ${
                          selectedMode === mode.slug
                            ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20 shadow-sm'
                            : mode.slug === agent.default_mode
                            ? 'border-green-300 dark:border-green-700 bg-green-50/50 dark:bg-green-900/10'
                            : 'border-gray-200 dark:border-gray-600 hover:border-gray-300 dark:hover:border-gray-500 hover:bg-white dark:hover:bg-gray-800'
                        }`}
                      >
                        <div className="flex items-center justify-between mb-1">
                          <span className="font-medium text-sm">{mode.name}</span>
                          {mode.slug === agent.default_mode && (
                            <span className="text-[10px] bg-green-100 dark:bg-green-800 text-green-700 dark:text-green-200 px-1.5 py-0.5 rounded-full">
                              default
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-gray-500 dark:text-gray-400 line-clamp-2 mb-2">
                          {mode.description}
                        </p>
                        <div className="flex flex-wrap gap-1">
                          {mode.tool_groups.map((tg: string) => (
                            <span
                              key={tg}
                              className={`text-[10px] px-1.5 py-0.5 rounded-full font-medium ${getModeToolBadge(tg)}`}
                            >
                              {tg}
                            </span>
                          ))}
                        </div>
                        {mode.recommended_extensions.length > 0 && (
                          <div className="mt-1.5 flex flex-wrap gap-1">
                            {mode.recommended_extensions.map((ext: string) => (
                              <span
                                key={ext}
                                className="text-[10px] px-1.5 py-0.5 rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300 border border-gray-200 dark:border-gray-600"
                              >
                                {ext}
                              </span>
                            ))}
                          </div>
                        )}
                      </button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          ))}
          {builtinAgents.length === 0 && (
            <div className="text-center py-8 text-gray-400">
              Loading builtin agents...
            </div>
          )}
        </div>
      </section>

      {/* External Agents Section */}
      <section>
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
            External Agents
          </h2>
          <button
            onClick={() => setShowConnect(!showConnect)}
            className="flex items-center gap-1.5 text-sm text-blue-500 hover:text-blue-600 transition-colors"
          >
            <Plus className="w-4 h-4" />
            Connect
          </button>
        </div>

        {showConnect && (
          <div className="mb-4 p-4 border border-gray-200 dark:border-gray-700 rounded-xl bg-gray-50 dark:bg-gray-800/30">
            <div className="flex gap-2">
              <input
                value={connectName}
                onChange={(e) => setConnectName(e.target.value)}
                placeholder="Agent name from registry..."
                className="flex-1 px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:outline-none focus:ring-2 focus:ring-blue-500"
                onKeyDown={(e) => e.key === 'Enter' && handleConnect()}
              />
              <button
                onClick={handleConnect}
                className="px-4 py-2 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors"
              >
                Connect
              </button>
            </div>
          </div>
        )}

        <div className="space-y-2">
          {externalAgents.map((agent) => (
            <div
              key={agent.id}
              className="flex items-center justify-between p-3 border border-gray-200 dark:border-gray-700 rounded-lg"
            >
              <div className="flex items-center gap-3">
                <Plug className="w-4 h-4 text-green-500" />
                <span className="font-medium text-sm">{agent.id}</span>
                <Circle className="w-2 h-2 fill-green-500 text-green-500" />
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => {
                    setPromptAgentId(agent.id);
                    setPromptResponse(null);
                  }}
                  className="p-1.5 rounded hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                  title="Prompt"
                >
                  <Play className="w-4 h-4" />
                </button>
                <button
                  onClick={() => handleDisconnect(agent.id)}
                  className="p-1.5 rounded hover:bg-red-50 dark:hover:bg-red-900/20 text-red-500 transition-colors"
                  title="Disconnect"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))}
          {externalAgents.length === 0 && !showConnect && (
            <div className="text-center py-6 text-gray-400 dark:text-gray-500 text-sm border border-dashed border-gray-300 dark:border-gray-600 rounded-xl">
              No external agents connected
            </div>
          )}
        </div>

        {/* Prompt Dialog */}
        {promptAgentId && (
          <div className="mt-4 p-4 border border-gray-200 dark:border-gray-700 rounded-xl bg-gray-50 dark:bg-gray-800/30">
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium">Prompt: {promptAgentId}</span>
              <button
                onClick={() => { setPromptAgentId(null); setPromptResponse(null); }}
                className="text-xs text-gray-400 hover:text-gray-600"
              >
                close
              </button>
            </div>
            <div className="flex gap-2 mb-2">
              <input
                value={promptText}
                onChange={(e) => setPromptText(e.target.value)}
                placeholder="Enter prompt..."
                className="flex-1 px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:outline-none focus:ring-2 focus:ring-blue-500"
                onKeyDown={(e) => e.key === 'Enter' && !prompting && handlePrompt(promptAgentId)}
              />
              <button
                onClick={() => handlePrompt(promptAgentId)}
                disabled={prompting}
                className="px-4 py-2 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 transition-colors"
              >
                {prompting ? 'Sending...' : 'Send'}
              </button>
            </div>
            {promptResponse && (
              <pre className="mt-2 p-3 bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg text-sm whitespace-pre-wrap max-h-48 overflow-y-auto">
                {promptResponse}
              </pre>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
