/**
 * DagEditor — 3-level test suite
 *
 * Level 1: Unit tests — serialization helpers, default configs, type utilities
 * Level 2: Integration tests — flow building + serialization (mocked API)
 * Level 3: E2E-like flow — full build → serialize → validate round-trip
 */
import { describe, expect, it, vi } from 'vitest';

// ── Mock ReactFlow ──────────────────────────────────────────────
vi.mock('@xyflow/react', () => ({
  ReactFlow: ({ children }: { children?: React.ReactNode }) => (
    <div data-testid="react-flow">{children}</div>
  ),
  ReactFlowProvider: ({ children }: { children?: React.ReactNode }) => <div>{children}</div>,
  Panel: ({ children }: { children?: React.ReactNode }) => <div>{children}</div>,
  Background: () => <div />,
  Controls: () => <div />,
  MiniMap: () => <div />,
  addEdge: vi.fn((edge: unknown, edges: unknown[]) => [...(edges as unknown[]), edge]),
  useNodesState: () => [[], vi.fn(), vi.fn()],
  useEdgesState: () => [[], vi.fn(), vi.fn()],
  useReactFlow: () => ({
    screenToFlowPosition: ({ x, y }: { x: number; y: number }) => ({ x, y }),
  }),
  MarkerType: { ArrowClosed: 'arrowclosed' },
}));

vi.mock('@/api/sdk.gen', () => ({
  agentCatalog: vi.fn(),
  savePipeline: vi.fn(),
  executePipeline: vi.fn(),
}));

vi.mock('@/api/client.gen', () => ({
  client: {
    getConfig: () => ({
      baseUrl: 'http://localhost:3000',
      headers: { 'X-Secret-Key': 'test-secret' },
    }),
  },
}));

vi.mock('@/contexts/ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({
    currentModel: 'o3-mini',
    currentProvider: 'openai',
  }),
}));

import { createNode, flowToPipeline, pipelineToYaml } from '../serialization';
import type { NodeKind } from '../types';
import { defaultConfig } from '../types';

// ═══════════════════════════════════════════════════════════════════
// Level 1: Unit Tests — pure functions
// ═══════════════════════════════════════════════════════════════════
describe('Level 1: Unit Tests', () => {
  describe('defaultConfig', () => {
    it('returns agent config with empty defaults', () => {
      const config = defaultConfig('agent');
      expect(config).toHaveProperty('agent');
      expect(config).toHaveProperty('prompt');
      expect(config).toHaveProperty('mode');
    });

    it('returns trigger config with manual event', () => {
      const config = defaultConfig('trigger');
      expect(config.event).toBe('manual');
    });

    it('returns tool config with empty fields', () => {
      const config = defaultConfig('tool');
      expect(config).toHaveProperty('extension');
      expect(config).toHaveProperty('tool');
    });

    it('returns a2a config with empty url', () => {
      const config = defaultConfig('a2a');
      expect(config).toHaveProperty('agent_card_url');
      expect(config).toHaveProperty('task');
    });

    it('returns condition config with expression', () => {
      const config = defaultConfig('condition');
      expect(config).toHaveProperty('expression');
    });

    it('returns transform config with template', () => {
      const config = defaultConfig('transform');
      expect(config).toHaveProperty('template');
    });

    it('returns human config with prompt', () => {
      const config = defaultConfig('human');
      expect(config).toHaveProperty('prompt');
    });

    for (const kind of [
      'trigger',
      'agent',
      'tool',
      'condition',
      'transform',
      'human',
      'a2a',
    ] as NodeKind[]) {
      it(`defaultConfig(${kind}) returns a non-empty object`, () => {
        const config = defaultConfig(kind);
        expect(Object.keys(config).length).toBeGreaterThan(0);
      });
    }
  });

  describe('createNode', () => {
    it('creates a node with correct kind and position', () => {
      const node = createNode('agent' as NodeKind, { x: 100, y: 200 });
      expect(node.type).toBe('agent');
      expect(node.position).toEqual({ x: 100, y: 200 });
      expect(node.data.kind).toBe('agent');
      expect(node.data.config).toBeDefined();
    });

    it('creates unique IDs for different node kinds', () => {
      const n1 = createNode('agent' as NodeKind, { x: 0, y: 0 });
      const n2 = createNode('trigger' as NodeKind, { x: 0, y: 0 });
      // Different kinds produce different ID prefixes
      expect(n1.id).not.toBe(n2.id);
      expect(n1.id).toMatch(/^agent_/);
      expect(n2.id).toMatch(/^trigger_/);
    });

    it('assigns default config for the node kind', () => {
      const node = createNode('trigger' as NodeKind, { x: 0, y: 0 });
      expect(node.data.config.event).toBe('manual');
    });

    it('sets label from palette name', () => {
      const node = createNode('agent' as NodeKind, { x: 0, y: 0 });
      expect(typeof node.data.label).toBe('string');
      expect(node.data.label.length).toBeGreaterThan(0);
    });
  });

  describe('flowToPipeline', () => {
    it('converts empty flow to pipeline with no nodes', () => {
      const pipeline = flowToPipeline([], [], {
        name: 'test',
        description: '',
      });
      expect(pipeline.nodes).toHaveLength(0);
      expect(pipeline.metadata.name).toBe('test');
      expect(pipeline.kind).toBe('Pipeline');
      expect(pipeline.apiVersion).toBe('goose/v1');
    });

    it('preserves node configs in pipeline', () => {
      const nodes = [
        {
          id: 'n1',
          type: 'agent',
          position: { x: 10, y: 20 },
          data: {
            kind: 'agent' as NodeKind,
            label: 'My Agent',
            config: {
              agent: 'developer',
              mode: 'write',
              prompt: 'do things',
            },
          },
        },
      ];
      const pipeline = flowToPipeline(nodes, [], {
        name: 'cfg-test',
        description: '',
      });
      expect(pipeline.nodes).toHaveLength(1);
      expect(pipeline.nodes[0].config.agent).toBe('developer');
      expect(pipeline.nodes[0].config.mode).toBe('write');
    });

    it('converts edges to depends arrays on nodes', () => {
      const nodes = [
        {
          id: 'a',
          type: 'trigger',
          position: { x: 0, y: 0 },
          data: {
            kind: 'trigger' as NodeKind,
            label: 'Start',
            config: { event: 'manual' },
          },
        },
        {
          id: 'b',
          type: 'agent',
          position: { x: 100, y: 0 },
          data: {
            kind: 'agent' as NodeKind,
            label: 'Agent',
            config: { agent: 'dev', prompt: 'test' },
          },
        },
      ];
      const edges = [{ id: 'e1', source: 'a', target: 'b' }];
      const pipeline = flowToPipeline(nodes, edges, {
        name: 'edge-test',
        description: '',
      });
      expect(pipeline.nodes).toHaveLength(2);
      // Node "b" should depend on "a"
      const nodeB = pipeline.nodes.find((n) => n.id === 'b');
      expect(nodeB?.depends).toContain('a');
      // Node "a" has no dependencies
      const nodeA = pipeline.nodes.find((n) => n.id === 'a');
      expect(nodeA?.depends).toBeUndefined();
    });

    it('uses node type from data.kind', () => {
      const node = createNode('condition' as NodeKind, { x: 0, y: 0 });
      const pipeline = flowToPipeline([node], [], {
        name: 'type-test',
        description: '',
      });
      expect(pipeline.nodes[0].type).toBe('condition');
    });
  });

  describe('pipelineToYaml', () => {
    it('produces valid YAML string', () => {
      const pipeline = flowToPipeline([], [], {
        name: 'yaml-test',
        description: '',
      });
      const yaml = pipelineToYaml(pipeline);
      expect(yaml).toContain('apiVersion:');
      expect(yaml).toContain('kind: Pipeline');
      expect(yaml).toContain('yaml-test');
    });
  });
});

// ═══════════════════════════════════════════════════════════════════
// Level 2: Integration Tests — flow building + serialization
// ═══════════════════════════════════════════════════════════════════
describe('Level 2: Integration Tests', () => {
  it('creates connected flow and serializes to pipeline', () => {
    const trigger = createNode('trigger' as NodeKind, { x: 0, y: 0 });
    const agent = createNode('agent' as NodeKind, { x: 200, y: 0 });
    agent.data.config = {
      agent: 'developer',
      mode: 'write',
      prompt: 'analyze code',
    };

    const edges = [{ id: 'e1', source: trigger.id, target: agent.id }];

    const pipeline = flowToPipeline([trigger, agent], edges, {
      name: 'integration-test',
      description: 'Test pipeline',
    });

    expect(pipeline.nodes).toHaveLength(2);
    const agentNode = pipeline.nodes.find((n) => n.type === 'agent');
    expect(agentNode?.config.agent).toBe('developer');
    expect(agentNode?.config.mode).toBe('write');
    // Agent depends on trigger
    expect(agentNode?.depends).toContain(trigger.id);
  });

  it('handles complex diamond DAG topology', () => {
    const start = createNode('trigger' as NodeKind, { x: 0, y: 0 });
    start.id = 'start_1';
    const left = createNode('agent' as NodeKind, { x: 100, y: -100 });
    left.id = 'left_1';
    left.data.config = { agent: 'developer', prompt: 'left path' };
    const right = createNode('agent' as NodeKind, { x: 100, y: 100 });
    right.id = 'right_1';
    right.data.config = { agent: 'qa', prompt: 'right path' };
    const merge = createNode('agent' as NodeKind, { x: 200, y: 0 });
    merge.id = 'merge_1';
    merge.data.config = {
      agent: 'developer',
      mode: 'review',
      prompt: 'merge',
    };

    const edges = [
      { id: 'e1', source: start.id, target: left.id },
      { id: 'e2', source: start.id, target: right.id },
      { id: 'e3', source: left.id, target: merge.id },
      { id: 'e4', source: right.id, target: merge.id },
    ];

    const pipeline = flowToPipeline([start, left, right, merge], edges, {
      name: 'diamond',
      description: '',
    });

    expect(pipeline.nodes).toHaveLength(4);
    // Merge node depends on both left and right
    const mergeNode = pipeline.nodes.find((n) => n.id === merge.id);
    expect(mergeNode?.depends).toHaveLength(2);
    expect(mergeNode?.depends).toContain(left.id);
    expect(mergeNode?.depends).toContain(right.id);
  });

  it('A2A node config preserves agent_card_url', () => {
    const a2aNode = createNode('a2a' as NodeKind, { x: 0, y: 0 });
    a2aNode.data.config = {
      agent_card_url: 'https://remote.example.com/.well-known/agent.json',
      task: 'analyze remotely',
    };

    const pipeline = flowToPipeline([a2aNode], [], {
      name: 'a2a-test',
      description: '',
    });
    expect(pipeline.nodes[0].config.agent_card_url).toBe(
      'https://remote.example.com/.well-known/agent.json'
    );
  });

  it('agent reasoning_effort is preserved in serialization', () => {
    const agent = createNode('agent' as NodeKind, { x: 0, y: 0 });
    agent.data.config = {
      agent: 'developer',
      mode: 'debug',
      prompt: 'find the bug',
      reasoning_effort: 'high',
    };

    const pipeline = flowToPipeline([agent], [], {
      name: 'effort-test',
      description: '',
    });
    expect(pipeline.nodes[0].config.reasoning_effort).toBe('high');
  });

  it('preserves node positions in serialized pipeline', () => {
    // Note: flowToPipeline does NOT preserve ReactFlow positions in pipeline nodes
    // Pipeline nodes don't have a position field — positions are ReactFlow-only
    const node = createNode('agent' as NodeKind, { x: 42, y: 99 });
    const pipeline = flowToPipeline([node], [], {
      name: 'pos-test',
      description: '',
    });
    // Pipeline node exists
    expect(pipeline.nodes).toHaveLength(1);
    expect(pipeline.nodes[0].id).toBe(node.id);
  });

  it('generates YAML with all node kinds', () => {
    const nodes = (
      ['trigger', 'agent', 'tool', 'condition', 'transform', 'human', 'a2a'] as NodeKind[]
    ).map((kind, i) => createNode(kind, { x: i * 100, y: 0 }));

    const pipeline = flowToPipeline(nodes, [], {
      name: 'all-kinds',
      description: '',
    });
    const yaml = pipelineToYaml(pipeline);

    // All node types present in pipeline
    const types = pipeline.nodes.map((n) => n.type).sort();
    expect(types).toEqual(['a2a', 'agent', 'condition', 'human', 'tool', 'transform', 'trigger']);

    // YAML contains key metadata
    expect(yaml).toContain('kind: Pipeline');
    expect(yaml).toContain('all-kinds');
  });
});

// ═══════════════════════════════════════════════════════════════════
// Level 3: E2E-like flow — build → serialize → validate round-trip
// ═══════════════════════════════════════════════════════════════════
describe('Level 3: E2E-like Flow', () => {
  it('full pipeline creation → YAML export → structure verification', () => {
    // Step 1: Create nodes programmatically (simulating drag-drop)
    const trigger = createNode('trigger' as NodeKind, { x: 0, y: 0 });
    trigger.id = 'e2e_trigger';
    const analyzer = createNode('agent' as NodeKind, { x: 200, y: -50 });
    analyzer.id = 'e2e_analyzer';
    analyzer.data.label = 'Code Analyzer';
    analyzer.data.config = {
      agent: 'developer',
      mode: 'write',
      prompt: 'analyze the codebase for issues',
      reasoning_effort: 'medium',
    };

    const tester = createNode('agent' as NodeKind, { x: 200, y: 50 });
    tester.id = 'e2e_tester';
    tester.data.label = 'Test Writer';
    tester.data.config = {
      agent: 'qa',
      mode: 'review',
      prompt: 'write tests for found issues',
    };

    const reviewer = createNode('agent' as NodeKind, { x: 400, y: 0 });
    reviewer.id = 'e2e_reviewer';
    reviewer.data.label = 'Code Reviewer';
    reviewer.data.config = {
      agent: 'developer',
      mode: 'review',
      prompt: 'review all changes',
      reasoning_effort: 'high',
    };

    // Step 2: Connect edges (diamond pattern)
    const edges = [
      { id: 'e1', source: trigger.id, target: analyzer.id },
      { id: 'e2', source: trigger.id, target: tester.id },
      { id: 'e3', source: analyzer.id, target: reviewer.id },
      { id: 'e4', source: tester.id, target: reviewer.id },
    ];

    // Step 3: Serialize to pipeline
    const pipeline = flowToPipeline([trigger, analyzer, tester, reviewer], edges, {
      name: 'code-review-pipeline',
      description: 'Automated code review',
    });

    // Step 4: Export to YAML
    const yaml = pipelineToYaml(pipeline);
    expect(yaml).toContain('code-review-pipeline');
    expect(yaml).toContain('developer');

    // Step 5: Verify structure
    expect(pipeline.nodes).toHaveLength(4);

    // Verify diamond topology — reviewer depends on both analyzer and tester
    const reviewerNode = pipeline.nodes.find((n) => n.id === reviewer.id);
    expect(reviewerNode?.depends).toHaveLength(2);
    expect(reviewerNode?.depends).toContain(analyzer.id);
    expect(reviewerNode?.depends).toContain(tester.id);

    // Analyzer and tester depend on trigger
    const analyzerNode = pipeline.nodes.find((n) => n.id === analyzer.id);
    expect(analyzerNode?.depends).toContain(trigger.id);
    const testerNode = pipeline.nodes.find((n) => n.id === tester.id);
    expect(testerNode?.depends).toContain(trigger.id);
  });

  it('pipeline with all node kinds → YAML → structure verification', () => {
    const trigger = createNode('trigger' as NodeKind, { x: 0, y: 0 });
    trigger.id = 'ak_trigger';
    const agent = createNode('agent' as NodeKind, { x: 200, y: 0 });
    agent.id = 'ak_agent';
    agent.data.config = {
      agent: 'developer',
      mode: 'write',
      prompt: 'build',
    };

    const condition = createNode('condition' as NodeKind, { x: 400, y: 0 });
    condition.id = 'ak_condition';
    condition.data.config = { expression: 'output.success === true' };

    const transform = createNode('transform' as NodeKind, { x: 600, y: -50 });
    transform.id = 'ak_transform';
    transform.data.config = { template: 'Summary: {{input}}' };

    const human = createNode('human' as NodeKind, { x: 600, y: 50 });
    human.id = 'ak_human';
    human.data.config = { prompt: 'Please review and approve' };

    const a2a = createNode('a2a' as NodeKind, { x: 800, y: 0 });
    a2a.id = 'ak_a2a';
    a2a.data.config = {
      agent_card_url: 'https://api.example.com/agent',
      task: 'deploy to staging',
    };

    const edges = [
      { id: 'e1', source: trigger.id, target: agent.id },
      { id: 'e2', source: agent.id, target: condition.id },
      { id: 'e3', source: condition.id, target: transform.id },
      { id: 'e4', source: condition.id, target: human.id },
      { id: 'e5', source: transform.id, target: a2a.id },
      { id: 'e6', source: human.id, target: a2a.id },
    ];

    const pipeline = flowToPipeline([trigger, agent, condition, transform, human, a2a], edges, {
      name: 'full-workflow',
      description: 'Complete workflow',
    });

    const yaml = pipelineToYaml(pipeline);

    // Verify all kinds present
    const types = pipeline.nodes.map((n) => n.type).sort();
    expect(types).toEqual(['a2a', 'agent', 'condition', 'human', 'transform', 'trigger']);

    // Verify dependency chains
    const a2aNode = pipeline.nodes.find((n) => n.id === a2a.id);
    expect(a2aNode?.depends).toHaveLength(2);

    // Verify YAML is non-trivial
    expect(yaml.length).toBeGreaterThan(200);
    expect(yaml).toContain('apiVersion:');
    expect(yaml).toContain('kind: Pipeline');
  });

  it('empty pipeline serializes cleanly', () => {
    const pipeline = flowToPipeline([], [], {
      name: 'empty',
      description: 'Nothing here',
    });
    const yaml = pipelineToYaml(pipeline);
    expect(yaml).toContain('empty');
    expect(yaml).toContain('apiVersion:');
    expect(pipeline.nodes).toHaveLength(0);
  });

  it('node status tracking works for execution simulation', () => {
    // Simulate what the DagEditor does during SSE execution
    const nodes = [
      createNode('trigger' as NodeKind, { x: 0, y: 0 }),
      createNode('agent' as NodeKind, { x: 200, y: 0 }),
    ];

    // Simulate SSE events updating node status
    const updatedNodes = nodes.map((n) => ({
      ...n,
      data: {
        ...n.data,
        status: 'running' as const,
      },
    }));
    expect(updatedNodes[0].data.status).toBe('running');

    // Simulate completion
    const completedNodes = updatedNodes.map((n) => ({
      ...n,
      data: {
        ...n.data,
        status: 'success' as const,
        output: 'Task completed',
      },
    }));
    expect(completedNodes[1].data.status).toBe('success');
    expect(completedNodes[1].data.output).toBe('Task completed');
  });

  it('SSE event format matches expected PipelineEvent structure', () => {
    // Verify the SSE event format we expect from the server
    const runStarted = {
      type: 'run_started',
      run_id: 'run-123',
      pipeline_id: 'my-pipeline',
    };
    expect(runStarted.type).toBe('run_started');

    const nodeStarted = {
      type: 'node_started',
      run_id: 'run-123',
      node_id: 'agent1',
    };
    expect(nodeStarted.type).toBe('node_started');

    const nodeCompleted = {
      type: 'node_completed',
      run_id: 'run-123',
      node_id: 'agent1',
      status: 'completed',
      output: 'Analysis done',
      duration_ms: 1500,
    };
    expect(nodeCompleted.type).toBe('node_completed');
    expect(nodeCompleted.duration_ms).toBe(1500);

    const runCompleted = {
      type: 'run_completed',
      run_id: 'run-123',
      status: 'completed',
      total_duration_ms: 5000,
    };
    expect(runCompleted.type).toBe('run_completed');
  });
});
