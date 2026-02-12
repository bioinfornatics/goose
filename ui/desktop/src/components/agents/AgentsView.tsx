import { useEffect, useState, useCallback } from 'react';
import { Bot, Plus, Trash2, Play, Settings2, RefreshCw } from 'lucide-react';
import {
  listAgents,
  connectAgent,
  disconnectAgent,
  createSession,
  promptAgent,
} from '../../api/sdk.gen';

interface ConnectedAgent {
  id: string;
  sessionId?: string;
  currentMode?: string;
}

export default function AgentsView() {
  const [agents, setAgents] = useState<ConnectedAgent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [showConnect, setShowConnect] = useState(false);
  const [connectName, setConnectName] = useState('');


  const [promptAgentId, setPromptAgentId] = useState<string | null>(null);
  const [promptText, setPromptText] = useState('');
  const [promptResponse, setPromptResponse] = useState<string | null>(null);
  const [prompting, setPrompting] = useState(false);

  const fetchAgents = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await listAgents();
      if (resp.data) {
        const ids: string[] = resp.data.agents || [];
        setAgents(ids.map((id) => ({ id })));
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to list agents');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchAgents();
  }, [fetchAgents]);

  const handleConnect = async () => {
    if (!connectName.trim()) return;
    setError(null);
    try {
      await connectAgent({
        body: {
          name: connectName,
        },
      });
      setShowConnect(false);
      setConnectName('');
      await fetchAgents();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to connect');
    }
  };

  const handleDisconnect = async (agentId: string) => {
    try {
      await disconnectAgent({ path: { agent_id: agentId } });
      await fetchAgents();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to disconnect');
    }
  };

  const handleCreateSession = async (agentId: string) => {
    try {
      const resp = await createSession({ path: { agent_id: agentId }, body: {} });
      if (resp.data) {
        setAgents((prev) =>
          prev.map((a) => (a.id === agentId ? { ...a, sessionId: resp.data?.session_id } : a))
        );
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to create session');
    }
  };

  const handlePrompt = async () => {
    if (!promptAgentId || !promptText.trim()) return;
    const agent = agents.find((a) => a.id === promptAgentId);
    if (!agent?.sessionId) {
      setError('Create a session first');
      return;
    }
    setPrompting(true);
    setPromptResponse(null);
    try {
      const resp = await promptAgent({
        path: { agent_id: promptAgentId },
        body: { session_id: agent.sessionId, text: promptText },
      });
      if (resp.data) {
        setPromptResponse(resp.data.text);
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to prompt');
    } finally {
      setPrompting(false);
    }
  };



  return (
    <div className="flex flex-col h-full p-6 overflow-y-auto">
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <Bot className="w-6 h-6 text-text-default" />
          <h1 className="text-2xl font-semibold text-text-default">External Agents</h1>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={fetchAgents}
            className="flex items-center gap-2 px-3 py-2 text-sm rounded-lg bg-background-medium hover:bg-background-medium/80 text-text-default transition-colors"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </button>
          <button
            onClick={() => setShowConnect(!showConnect)}
            className="flex items-center gap-2 px-3 py-2 text-sm rounded-lg bg-accent-primary text-text-on-accent hover:bg-accent-primary/80 transition-colors"
          >
            <Plus className="w-4 h-4" />
            Connect Agent
          </button>
        </div>
      </div>

      {error && (
        <div className="mb-4 p-3 rounded-lg bg-red-100 dark:bg-red-900/20 text-red-800 dark:text-red-200 text-sm">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">
            Dismiss
          </button>
        </div>
      )}

      {showConnect && (
        <div className="mb-6 p-4 rounded-lg bg-background-default border border-border-default">
          <h3 className="text-sm font-medium text-text-default mb-3">Connect to Agent</h3>
          <div className="flex flex-col gap-3">
            <input
              type="text"
              value={connectName}
              onChange={(e) => setConnectName(e.target.value)}
              placeholder="Agent name (e.g., code-reviewer)"
              className="px-3 py-2 rounded-lg bg-background-muted border border-border-default text-text-default text-sm focus:outline-none focus:ring-2 focus:ring-accent-primary"
            />

            <div className="flex gap-2">
              <button
                onClick={handleConnect}
                className="px-4 py-2 text-sm rounded-lg bg-accent-primary text-text-on-accent hover:bg-accent-primary/80"
              >
                Connect
              </button>
              <button
                onClick={() => setShowConnect(false)}
                className="px-4 py-2 text-sm rounded-lg bg-background-medium text-text-default hover:bg-background-medium/80"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {agents.length === 0 && !loading ? (
        <div className="flex flex-col items-center justify-center py-16 text-text-muted">
          <Bot className="w-12 h-12 mb-4 opacity-50" />
          <p className="text-lg font-medium">No external agents connected</p>
          <p className="text-sm mt-1">Connect an agent to start delegating tasks via ACP</p>
        </div>
      ) : (
        <div className="grid gap-4">
          {agents.map((agent) => (
            <div
              key={agent.id}
              className="p-4 rounded-lg bg-background-default border border-border-default hover:border-accent-primary/50 transition-colors"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="w-2 h-2 rounded-full bg-green-500" />
                  <span className="font-medium text-text-default">{agent.id}</span>
                  {agent.sessionId && (
                    <span className="text-xs px-2 py-0.5 rounded-full bg-accent-primary/10 text-accent-primary">
                      Session active
                    </span>
                  )}
                  {agent.currentMode && (
                    <span className="text-xs px-2 py-0.5 rounded-full bg-purple-100 dark:bg-purple-900/20 text-purple-800 dark:text-purple-200">
                      Mode: {agent.currentMode}
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  {!agent.sessionId && (
                    <button
                      onClick={() => handleCreateSession(agent.id)}
                      className="p-2 rounded-lg hover:bg-background-medium text-text-muted hover:text-text-default transition-colors"
                      title="Create session"
                    >
                      <Play className="w-4 h-4" />
                    </button>
                  )}
                  {agent.sessionId && (
                    <button
                      onClick={() =>
                        setPromptAgentId(promptAgentId === agent.id ? null : agent.id)
                      }
                      className="p-2 rounded-lg hover:bg-background-medium text-text-muted hover:text-text-default transition-colors"
                      title="Send prompt"
                    >
                      <Settings2 className="w-4 h-4" />
                    </button>
                  )}
                  <button
                    onClick={() => handleDisconnect(agent.id)}
                    className="p-2 rounded-lg hover:bg-red-100 dark:hover:bg-red-900/20 text-text-muted hover:text-red-600 transition-colors"
                    title="Disconnect"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>

              {promptAgentId === agent.id && agent.sessionId && (
                <div className="mt-4 pt-4 border-t border-border-default">
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={promptText}
                      onChange={(e) => setPromptText(e.target.value)}
                      onKeyDown={(e) => e.key === 'Enter' && handlePrompt()}
                      placeholder="Send a prompt to this agent..."
                      className="flex-1 px-3 py-2 rounded-lg bg-background-muted border border-border-default text-text-default text-sm focus:outline-none focus:ring-2 focus:ring-accent-primary"
                      disabled={prompting}
                    />
                    <button
                      onClick={handlePrompt}
                      disabled={prompting || !promptText.trim()}
                      className="px-4 py-2 text-sm rounded-lg bg-accent-primary text-text-on-accent hover:bg-accent-primary/80 disabled:opacity-50"
                    >
                      {prompting ? 'Sending...' : 'Send'}
                    </button>
                  </div>
                  {promptResponse && (
                    <div className="mt-3 p-3 rounded-lg bg-background-muted text-text-default text-sm whitespace-pre-wrap">
                      {promptResponse}
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
