import type { Edge, Node } from '@xyflow/react';
import { Bot, FileText, GitBranch, Globe, Search, Zap } from 'lucide-react';
import { useState } from 'react';
import { createNode } from '../serialization';
import type { DagNodeData, PipelineMetadata } from '../types';

// ── Template definitions ────────────────────────────────────────────

export interface PipelineTemplate {
  id: string;
  name: string;
  description: string;
  category: 'development' | 'research' | 'automation' | 'review';
  icon: string;
  build: () => { nodes: Node<DagNodeData>[]; edges: Edge[]; metadata: PipelineMetadata };
}

let nodeCounter = 0;
function uid(kind: string): string {
  nodeCounter += 1;
  return `${kind}_tpl_${nodeCounter}`;
}

function makeNode(
  kind: string,
  label: string,
  x: number,
  y: number,
  config: Record<string, unknown>
): Node<DagNodeData> {
  const node = createNode(kind as never, { x, y });
  node.id = uid(kind);
  node.data = { ...node.data, label, config: { ...node.data.config, ...config } as never };
  return node;
}

function makeEdge(source: string, target: string, label?: string): Edge {
  return { id: `e_${source}_${target}`, source, target, label, type: 'smoothstep' };
}

export const PIPELINE_TEMPLATES: PipelineTemplate[] = [
  {
    id: 'code-review',
    name: 'Code Review Pipeline',
    description: 'Analyze code, run tests, review quality, and generate a report',
    category: 'review',
    icon: 'GitBranch',
    build() {
      nodeCounter = 0;
      const trigger = makeNode('trigger', 'On PR', 0, 0, { event: 'manual' });
      const analyze = makeNode('agent', 'Analyze Code', 250, -80, {
        agent: 'Developer',
        mode: 'ask',
        prompt: 'Analyze the code changes for potential issues, patterns, and complexity.',
      });
      const test = makeNode('agent', 'Run Tests', 250, 80, {
        agent: 'Developer',
        mode: 'write',
        prompt: 'Run the test suite and report any failures.',
      });
      const review = makeNode('agent', 'Quality Review', 500, 0, {
        agent: 'QA',
        mode: 'review',
        prompt:
          'Review the analysis and test results. Provide a quality assessment with actionable feedback.',
        reasoning_effort: 'high',
      });
      const report = makeNode('transform', 'Generate Report', 750, 0, {
        template: 'Combine analysis and review into a structured code review report.',
      });

      return {
        nodes: [trigger, analyze, test, review, report],
        edges: [
          makeEdge(trigger.id, analyze.id),
          makeEdge(trigger.id, test.id),
          makeEdge(analyze.id, review.id),
          makeEdge(test.id, review.id),
          makeEdge(review.id, report.id),
        ],
        metadata: {
          name: 'Code Review Pipeline',
          description: 'Automated code review with analysis, testing, and quality assessment',
        },
      };
    },
  },
  {
    id: 'research-pipeline',
    name: 'Research Pipeline',
    description: 'Research a topic, analyze findings, and produce a summary report',
    category: 'research',
    icon: 'Search',
    build() {
      nodeCounter = 0;
      const trigger = makeNode('trigger', 'Start Research', 0, 0, { event: 'manual' });
      const research = makeNode('agent', 'Deep Research', 250, 0, {
        agent: 'Research',
        mode: 'research',
        prompt:
          'Research the given topic thoroughly. Find key sources, compare perspectives, identify trends.',
        reasoning_effort: 'high',
      });
      const analyze = makeNode('agent', 'Analyze Findings', 500, -80, {
        agent: 'Developer',
        mode: 'plan',
        prompt: 'Analyze the research findings. Identify key insights, contradictions, and gaps.',
      });
      const summarize = makeNode('transform', 'Summarize', 500, 80, {
        template: 'Create a concise executive summary of the research.',
      });
      const report = makeNode('agent', 'Write Report', 750, 0, {
        agent: 'Developer',
        mode: 'write',
        prompt:
          'Compile the analysis and summary into a well-structured research report with citations.',
      });

      return {
        nodes: [trigger, research, analyze, summarize, report],
        edges: [
          makeEdge(trigger.id, research.id),
          makeEdge(research.id, analyze.id),
          makeEdge(research.id, summarize.id),
          makeEdge(analyze.id, report.id),
          makeEdge(summarize.id, report.id),
        ],
        metadata: {
          name: 'Research Pipeline',
          description: 'End-to-end research workflow with analysis and report generation',
        },
      };
    },
  },
  {
    id: 'bug-triage',
    name: 'Bug Triage & Fix',
    description: 'Analyze a bug report, reproduce it, find the root cause, and implement a fix',
    category: 'development',
    icon: 'Zap',
    build() {
      nodeCounter = 0;
      const trigger = makeNode('trigger', 'Bug Report', 0, 0, { event: 'manual' });
      const analyze = makeNode('agent', 'Analyze Bug', 250, 0, {
        agent: 'Developer',
        mode: 'debug',
        prompt:
          'Analyze the bug report. Identify the component, expected behavior, and reproduction steps.',
        reasoning_effort: 'high',
      });
      const canReproduce = makeNode('condition', 'Reproducible?', 500, 0, {
        expression: 'Can the bug be reliably reproduced?',
      });
      const fix = makeNode('agent', 'Implement Fix', 750, -80, {
        agent: 'Developer',
        mode: 'write',
        prompt: 'Implement a fix for the identified bug. Include tests to prevent regression.',
      });
      const needsInfo = makeNode('human', 'Request Info', 750, 80, {
        prompt: 'The bug could not be reproduced. Please provide additional information.',
      });
      const verify = makeNode('agent', 'Verify Fix', 1000, -80, {
        agent: 'QA',
        mode: 'review',
        prompt: 'Verify the fix resolves the bug and doesnt introduce regressions.',
      });

      return {
        nodes: [trigger, analyze, canReproduce, fix, needsInfo, verify],
        edges: [
          makeEdge(trigger.id, analyze.id),
          makeEdge(analyze.id, canReproduce.id),
          makeEdge(canReproduce.id, fix.id, 'yes'),
          makeEdge(canReproduce.id, needsInfo.id, 'no'),
          makeEdge(fix.id, verify.id),
        ],
        metadata: {
          name: 'Bug Triage & Fix',
          description: 'Automated bug analysis, reproduction, fix, and verification',
        },
      };
    },
  },
  {
    id: 'multi-agent-review',
    name: 'Multi-Agent Review',
    description: 'Parallel review by Security, QA, and PM agents with result aggregation',
    category: 'review',
    icon: 'Bot',
    build() {
      nodeCounter = 0;
      const trigger = makeNode('trigger', 'Submit for Review', 0, 0, { event: 'manual' });
      const security = makeNode('agent', 'Security Review', 250, -120, {
        agent: 'Security',
        mode: 'audit',
        prompt:
          'Perform a security audit. Check for vulnerabilities, data exposure, and compliance issues.',
        reasoning_effort: 'high',
      });
      const qa = makeNode('agent', 'QA Review', 250, 0, {
        agent: 'QA',
        mode: 'review',
        prompt:
          'Review for quality: test coverage, edge cases, error handling, and code standards.',
      });
      const pm = makeNode('agent', 'PM Review', 250, 120, {
        agent: 'PM',
        mode: 'review',
        prompt:
          'Review from a product perspective: does this meet the requirements and user expectations?',
      });
      const aggregate = makeNode('transform', 'Aggregate Reviews', 550, 0, {
        template:
          'Combine all review feedback into a unified review document with priority ranking.',
      });
      const decide = makeNode('condition', 'Approved?', 800, 0, {
        expression: 'Are all reviews passed without critical issues?',
      });

      return {
        nodes: [trigger, security, qa, pm, aggregate, decide],
        edges: [
          makeEdge(trigger.id, security.id),
          makeEdge(trigger.id, qa.id),
          makeEdge(trigger.id, pm.id),
          makeEdge(security.id, aggregate.id),
          makeEdge(qa.id, aggregate.id),
          makeEdge(pm.id, aggregate.id),
          makeEdge(aggregate.id, decide.id),
        ],
        metadata: {
          name: 'Multi-Agent Review',
          description: 'Parallel security, QA, and PM review with aggregated results',
        },
      };
    },
  },
  {
    id: 'a2a-delegation',
    name: 'A2A Delegation',
    description: 'Delegate tasks to remote agents via A2A protocol',
    category: 'automation',
    icon: 'Globe',
    build() {
      nodeCounter = 0;
      const trigger = makeNode('trigger', 'Start', 0, 0, { event: 'manual' });
      const plan = makeNode('agent', 'Plan Tasks', 250, 0, {
        agent: 'Developer',
        mode: 'plan',
        prompt: 'Break down the task into sub-tasks suitable for delegation.',
      });
      const local = makeNode('agent', 'Local Processing', 500, -80, {
        agent: 'Developer',
        mode: 'write',
        prompt: 'Handle the local processing component of the task.',
      });
      const remote = makeNode('a2a', 'Remote Agent', 500, 80, {
        agent_card_url: '',
        task: 'Handle the delegated sub-task according to your capabilities.',
      });
      const merge = makeNode('transform', 'Merge Results', 750, 0, {
        template: 'Merge local and remote results into a unified output.',
      });

      return {
        nodes: [trigger, plan, local, remote, merge],
        edges: [
          makeEdge(trigger.id, plan.id),
          makeEdge(plan.id, local.id),
          makeEdge(plan.id, remote.id),
          makeEdge(local.id, merge.id),
          makeEdge(remote.id, merge.id),
        ],
        metadata: {
          name: 'A2A Delegation',
          description: 'Hybrid local + remote agent task delegation via A2A protocol',
        },
      };
    },
  },
];

// ── Category metadata ────────────────────────────────────────────────

const CATEGORY_ICONS: Record<string, React.FC<{ size?: number }>> = {
  GitBranch,
  Search,
  Zap,
  Bot,
  Globe,
};

const CATEGORY_COLORS: Record<string, string> = {
  development: '#3b82f6',
  research: '#8b5cf6',
  automation: '#10b981',
  review: '#f59e0b',
};

const CATEGORY_LABELS: Record<string, string> = {
  development: 'Development',
  research: 'Research',
  automation: 'Automation',
  review: 'Review',
};

// ── Component ────────────────────────────────────────────────────────

interface TemplateGalleryProps {
  onSelect: (template: PipelineTemplate) => void;
}

export function TemplateGallery({ onSelect }: TemplateGalleryProps) {
  const [search, setSearch] = useState('');
  const [activeCategory, setActiveCategory] = useState<string | null>(null);

  const categories = [...new Set(PIPELINE_TEMPLATES.map((t) => t.category))];

  const filtered = PIPELINE_TEMPLATES.filter((t) => {
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
        <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-subtle" />
        <input
          type="text"
          placeholder="Search templates..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="w-full pl-8 pr-3 py-1.5 text-xs bg-background-default border border-border-default rounded-md text-text-default placeholder-text-subtle focus:outline-none focus:border-border-accent"
        />
      </div>

      {/* Category filter pills */}
      <div className="flex flex-wrap gap-1">
        <button
          type="button"
          onClick={() => setActiveCategory(null)}
          className={`px-2 py-0.5 text-xs rounded-full transition-colors ${
            !activeCategory
              ? 'bg-background-accent text-text-on-accent'
              : 'bg-background-muted text-text-muted hover:bg-background-default'
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
                : 'bg-background-muted text-text-muted hover:bg-background-default'
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
              className="w-full text-left p-2.5 rounded-lg border border-border-default hover:border-border-accent hover:bg-background-muted transition-all group"
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
                  <div className="text-sm font-medium text-text-default group-hover:text-text-accent truncate">
                    {template.name}
                  </div>
                  <div className="text-xs text-text-muted mt-0.5 line-clamp-2">
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
                  </div>
                </div>
              </div>
            </button>
          );
        })}
        {filtered.length === 0 && (
          <div className="text-center py-6 text-xs text-text-muted">
            No templates match your search
          </div>
        )}
      </div>
    </div>
  );
}
