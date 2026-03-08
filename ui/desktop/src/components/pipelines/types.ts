/**
 * Pipeline editor types — extends the generated API types with
 * editor-specific data (ReactFlow node data, palette items, etc.).
 */
import type { NodeKind } from '../../api/types.gen';

// ── Node configuration types ───────────────────────────────────────

export interface TriggerConfig {
  event: string;
}

export interface AgentConfig {
  agent: string;
  mode: string;
  prompt: string;
}

export interface ToolConfig {
  extension: string;
  tool: string;
  arguments: Record<string, unknown>;
}

export interface ConditionConfig {
  expression: string;
}

export interface TransformConfig {
  template: string;
}

export interface HumanConfig {
  prompt: string;
  timeout: number;
  default_action: string;
}

export interface A2aConfig {
  agent_card_url: string;
  task: string;
}

export type NodeConfig =
  | TriggerConfig
  | AgentConfig
  | ToolConfig
  | ConditionConfig
  | TransformConfig
  | HumanConfig
  | A2aConfig;

// ── ReactFlow node data ────────────────────────────────────────────

export interface DagNodeData {
  kind: NodeKind;
  label: string;
  config: NodeConfig;
  condition?: string;
  status?: 'idle' | 'running' | 'success' | 'error' | 'skipped';
  output?: string;
  [key: string]: unknown;
}

// ── Palette items ──────────────────────────────────────────────────

export interface PaletteItem {
  kind: NodeKind;
  label: string;
  description: string;
  icon: string;
  color: string;
}

export const NODE_PALETTE: PaletteItem[] = [
  {
    kind: 'trigger',
    label: 'Trigger',
    description: 'Entry point for the pipeline',
    icon: 'Zap',
    color: '#6366f1',
  },
  {
    kind: 'agent',
    label: 'Agent',
    description: 'Run a Goose agent with a prompt',
    icon: 'Bot',
    color: '#8b5cf6',
  },
  {
    kind: 'tool',
    label: 'Tool',
    description: 'Call a specific MCP tool',
    icon: 'Wrench',
    color: '#0ea5e9',
  },
  {
    kind: 'condition',
    label: 'Condition',
    description: 'Boolean gate or branch',
    icon: 'GitBranch',
    color: '#f59e0b',
  },
  {
    kind: 'transform',
    label: 'Transform',
    description: 'Data transformation template',
    icon: 'ArrowRightLeft',
    color: '#10b981',
  },
  {
    kind: 'human',
    label: 'Human',
    description: 'Human-in-the-loop approval',
    icon: 'UserCheck',
    color: '#ec4899',
  },
  {
    kind: 'a2a',
    label: 'A2A Agent',
    description: 'Call an external A2A agent',
    icon: 'Globe',
    color: '#14b8a6',
  },
];

// ── Default configs ────────────────────────────────────────────────

export function defaultConfig(kind: NodeKind): NodeConfig {
  switch (kind) {
    case 'trigger':
      return { event: 'manual' } as TriggerConfig;
    case 'agent':
      return { agent: '', mode: '', prompt: '' } as AgentConfig;
    case 'tool':
      return { extension: '', tool: '', arguments: {} } as ToolConfig;
    case 'condition':
      return { expression: '' } as ConditionConfig;
    case 'transform':
      return { template: '' } as TransformConfig;
    case 'human':
      return { prompt: '', timeout: 300, default_action: 'skip' } as HumanConfig;
    case 'a2a':
      return { agent_card_url: '', task: '' } as A2aConfig;
  }
}

// ── Pipeline templates ─────────────────────────────────────────────

export type TemplateCategory = 'automation' | 'analysis' | 'devops' | 'integration';

export interface PipelineTemplate {
  id: string;
  name: string;
  description: string;
  category: TemplateCategory;
  icon: string;
  nodeCount: number;
  buildNodes: () => {
    nodes: Array<{ id: string; kind: NodeKind; label: string; config: unknown; position?: { x: number; y: number } }>;
    edges: Array<{ source: string; target: string; label?: string }>;
  };
}
