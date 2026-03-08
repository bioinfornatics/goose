/**
 * TemplateGallery — browse and apply pre-built pipeline templates.
 *
 * Adapted from feature/cli-via-goosed to use the API Pipeline type.
 */
import { Bot, FileText, GitBranch, Globe, Search, Zap } from 'lucide-react';
import { useState } from 'react';
import type { PipelineTemplate, TemplateCategory } from './types';

// ── Template library ───────────────────────────────────────────────

const TEMPLATES: PipelineTemplate[] = [
  {
    id: 'simple-agent',
    name: 'Simple Agent Pipeline',
    description: 'A trigger that kicks off a single agent task — the "hello world" of pipelines.',
    category: 'automation',
    icon: 'Bot',
    nodeCount: 2,
    buildNodes: () => ({
      nodes: [
        { id: 'trigger_1', kind: 'trigger', label: 'Start', config: { event: 'manual' }, position: { x: 250, y: 50 } },
        { id: 'agent_1', kind: 'agent', label: 'Run Agent', config: { agent: '', mode: '', prompt: 'Describe the task here' }, position: { x: 250, y: 200 } },
      ],
      edges: [{ source: 'trigger_1', target: 'agent_1' }],
    }),
  },
  {
    id: 'code-review',
    name: 'Code Review Pipeline',
    description: 'Trigger → agent writes code → condition checks quality → human reviews if needed.',
    category: 'devops',
    icon: 'GitBranch',
    nodeCount: 4,
    buildNodes: () => ({
      nodes: [
        { id: 'trigger_1', kind: 'trigger', label: 'PR Opened', config: { event: 'webhook' }, position: { x: 250, y: 50 } },
        { id: 'agent_1', kind: 'agent', label: 'Analyze Code', config: { agent: '', mode: '', prompt: 'Review the PR for bugs and style issues' }, position: { x: 250, y: 200 } },
        { id: 'cond_1', kind: 'condition', label: 'Issues Found?', config: { expression: 'result.issues.length > 0' }, position: { x: 250, y: 350 } },
        { id: 'human_1', kind: 'human', label: 'Manual Review', config: { prompt: 'Review flagged issues', timeout: 600, default_action: 'skip' }, position: { x: 450, y: 500 } },
      ],
      edges: [
        { source: 'trigger_1', target: 'agent_1' },
        { source: 'agent_1', target: 'cond_1' },
        { source: 'cond_1', target: 'human_1', label: 'true' },
      ],
    }),
  },
  {
    id: 'data-analysis',
    name: 'Data Analysis Pipeline',
    description: 'Fetch data → transform → analyze with an agent → produce a report.',
    category: 'analysis',
    icon: 'Search',
    nodeCount: 4,
    buildNodes: () => ({
      nodes: [
        { id: 'trigger_1', kind: 'trigger', label: 'Schedule', config: { event: 'cron' }, position: { x: 250, y: 50 } },
        { id: 'tool_1', kind: 'tool', label: 'Fetch Data', config: { extension: '', tool: '', arguments: {} }, position: { x: 250, y: 200 } },
        { id: 'transform_1', kind: 'transform', label: 'Clean & Format', config: { template: '{{ data | filter | sort }}' }, position: { x: 250, y: 350 } },
        { id: 'agent_1', kind: 'agent', label: 'Generate Report', config: { agent: '', mode: '', prompt: 'Analyze the data and produce a summary report' }, position: { x: 250, y: 500 } },
      ],
      edges: [
        { source: 'trigger_1', target: 'tool_1' },
        { source: 'tool_1', target: 'transform_1' },
        { source: 'transform_1', target: 'agent_1' },
      ],
    }),
  },
  {
    id: 'a2a-collaboration',
    name: 'Multi-Agent Collaboration',
    description: 'Coordinate work between a local agent and a remote A2A agent.',
    category: 'integration',
    icon: 'Globe',
    nodeCount: 4,
    buildNodes: () => ({
      nodes: [
        { id: 'trigger_1', kind: 'trigger', label: 'Start', config: { event: 'manual' }, position: { x: 250, y: 50 } },
        { id: 'agent_1', kind: 'agent', label: 'Plan Work', config: { agent: '', mode: '', prompt: 'Break down the task into sub-tasks' }, position: { x: 250, y: 200 } },
        { id: 'a2a_1', kind: 'a2a', label: 'Remote Agent', config: { agent_card_url: '', task: 'Execute delegated sub-task' }, position: { x: 250, y: 350 } },
        { id: 'agent_2', kind: 'agent', label: 'Summarize', config: { agent: '', mode: '', prompt: 'Combine results from all agents' }, position: { x: 250, y: 500 } },
      ],
      edges: [
        { source: 'trigger_1', target: 'agent_1' },
        { source: 'agent_1', target: 'a2a_1' },
        { source: 'a2a_1', target: 'agent_2' },
      ],
    }),
  },
  {
    id: 'ci-pipeline',
    name: 'CI/CD Pipeline',
    description: 'Build → test → conditional deploy with human approval gate.',
    category: 'devops',
    icon: 'Zap',
    nodeCount: 5,
    buildNodes: () => ({
      nodes: [
        { id: 'trigger_1', kind: 'trigger', label: 'Push to Main', config: { event: 'webhook' }, position: { x: 250, y: 50 } },
        { id: 'tool_1', kind: 'tool', label: 'Run Tests', config: { extension: '', tool: 'run_tests', arguments: {} }, position: { x: 250, y: 200 } },
        { id: 'cond_1', kind: 'condition', label: 'Tests Pass?', config: { expression: 'result.exit_code === 0' }, position: { x: 250, y: 350 } },
        { id: 'human_1', kind: 'human', label: 'Approve Deploy', config: { prompt: 'All tests passed. Deploy to production?', timeout: 3600, default_action: 'skip' }, position: { x: 100, y: 500 } },
        { id: 'tool_2', kind: 'tool', label: 'Deploy', config: { extension: '', tool: 'deploy', arguments: { env: 'production' } }, position: { x: 100, y: 650 } },
      ],
      edges: [
        { source: 'trigger_1', target: 'tool_1' },
        { source: 'tool_1', target: 'cond_1' },
        { source: 'cond_1', target: 'human_1', label: 'true' },
        { source: 'human_1', target: 'tool_2' },
      ],
    }),
  },
  {
    id: 'research-digest',
    name: 'Research Digest',
    description: 'Fetch articles → summarize each → compile a digest report.',
    category: 'analysis',
    icon: 'FileText',
    nodeCount: 3,
    buildNodes: () => ({
      nodes: [
        { id: 'trigger_1', kind: 'trigger', label: 'Daily Schedule', config: { event: 'cron' }, position: { x: 250, y: 50 } },
        { id: 'agent_1', kind: 'agent', label: 'Summarize Articles', config: { agent: '', mode: '', prompt: 'Fetch and summarize the latest research papers on the topic' }, position: { x: 250, y: 200 } },
        { id: 'agent_2', kind: 'agent', label: 'Compile Digest', config: { agent: '', mode: '', prompt: 'Combine summaries into a well-structured digest' }, position: { x: 250, y: 350 } },
      ],
      edges: [
        { source: 'trigger_1', target: 'agent_1' },
        { source: 'agent_1', target: 'agent_2' },
      ],
    }),
  },
];

// ── Category metadata ──────────────────────────────────────────────

const CATEGORY_LABELS: Record<TemplateCategory, string> = {
  automation: 'Automation',
  analysis: 'Analysis',
  devops: 'DevOps',
  integration: 'Integration',
};

const CATEGORY_COLORS: Record<TemplateCategory, string> = {
  automation: '#6366f1',
  analysis: '#0ea5e9',
  devops: '#f59e0b',
  integration: '#10b981',
};

const CATEGORY_ICONS: Record<string, typeof Bot> = {
  Bot,
  Zap,
  Search,
  GitBranch,
  Globe,
  FileText,
};

const categories: TemplateCategory[] = ['automation', 'analysis', 'devops', 'integration'];

// ── Component ──────────────────────────────────────────────────────

interface TemplateGalleryProps {
  onSelect: (template: PipelineTemplate) => void;
}

export function TemplateGallery({ onSelect }: TemplateGalleryProps) {
  const [search, setSearch] = useState('');
  const [activeCategory, setActiveCategory] = useState<TemplateCategory | null>(null);

  const filtered = TEMPLATES.filter((t) => {
    const matchesSearch =
      !search ||
      t.name.toLowerCase().includes(search.toLowerCase()) ||
      t.description.toLowerCase().includes(search.toLowerCase());
    const matchesCategory = !activeCategory || t.category === activeCategory;
    return matchesSearch && matchesCategory;
  });

  return (
    <div className="space-y-3">
      {/* Search */}
      <div className="relative">
        <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-400" />
        <input
          type="text"
          placeholder="Search templates..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="w-full pl-8 pr-3 py-1.5 text-xs border rounded-md focus:outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>

      {/* Category filter */}
      <div className="flex flex-wrap gap-1">
        <button
          type="button"
          onClick={() => setActiveCategory(null)}
          className={`px-2 py-0.5 text-xs rounded-full transition-colors ${
            !activeCategory
              ? 'bg-blue-500 text-white'
              : 'bg-gray-100 text-gray-500 hover:bg-gray-200'
          }`}
        >
          All
        </button>
        {categories.map((cat) => (
          <button
            key={cat}
            type="button"
            onClick={() => setActiveCategory(activeCategory === cat ? null : cat)}
            className={`px-2 py-0.5 text-xs rounded-full transition-colors ${
              activeCategory === cat
                ? 'text-white'
                : 'bg-gray-100 text-gray-500 hover:bg-gray-200'
            }`}
            style={activeCategory === cat ? { backgroundColor: CATEGORY_COLORS[cat] } : undefined}
          >
            {CATEGORY_LABELS[cat]}
          </button>
        ))}
      </div>

      {/* Template cards */}
      <div className="space-y-1.5 max-h-[calc(100vh-300px)] overflow-y-auto">
        {filtered.map((template) => {
          const Icon = CATEGORY_ICONS[template.icon] || FileText;
          return (
            <button
              key={template.id}
              type="button"
              onClick={() => onSelect(template)}
              className="w-full text-left p-2.5 rounded-lg border hover:border-blue-300 hover:bg-gray-50 transition-all group"
            >
              <div className="flex items-start gap-2.5">
                <div
                  className="flex items-center justify-center w-8 h-8 rounded-md shrink-0 mt-0.5"
                  style={{
                    backgroundColor: `${CATEGORY_COLORS[template.category]}15`,
                    color: CATEGORY_COLORS[template.category],
                  }}
                >
                  <Icon size={16} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium group-hover:text-blue-600 truncate">
                    {template.name}
                  </div>
                  <div className="text-xs text-gray-500 mt-0.5 line-clamp-2">
                    {template.description}
                  </div>
                  <div className="flex items-center gap-2 mt-1.5">
                    <span
                      className="text-[10px] px-1.5 py-0.5 rounded-full"
                      style={{
                        backgroundColor: `${CATEGORY_COLORS[template.category]}15`,
                        color: CATEGORY_COLORS[template.category],
                      }}
                    >
                      {CATEGORY_LABELS[template.category]}
                    </span>
                    <span className="text-[10px] text-gray-400">
                      {template.nodeCount} nodes
                    </span>
                  </div>
                </div>
              </div>
            </button>
          );
        })}
        {filtered.length === 0 && (
          <div className="text-center py-6 text-xs text-gray-400">
            No templates match your search
          </div>
        )}
      </div>
    </div>
  );
}
