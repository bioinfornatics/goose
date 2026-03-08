/**
 * Pipeline ↔ ReactFlow serialization helpers.
 *
 * Converts between the API Pipeline type (flat nodes/edges) and
 * ReactFlow's Node<DagNodeData>/Edge representation.
 */
import type { Edge, Node } from '@xyflow/react';
import type { NodeKind, Pipeline, PipelineEdge, PipelineNode } from '../../api/types.gen';
import type { DagNodeData, NodeConfig } from './types';
import { defaultConfig } from './types';

// ── API → ReactFlow ────────────────────────────────────────────────

/** Convert an API PipelineNode to a ReactFlow Node<DagNodeData>. */
export function apiNodeToRF(node: PipelineNode, index: number): Node<DagNodeData> {
  return {
    id: node.id,
    type: node.kind,
    position: node.position ?? { x: 250, y: index * 120 },
    data: {
      kind: node.kind,
      label: node.label,
      config: (node.config ?? defaultConfig(node.kind)) as NodeConfig,
      condition: node.condition ?? undefined,
      status: 'idle',
    },
  };
}

/** Convert an API PipelineEdge to a ReactFlow Edge. */
export function apiEdgeToRF(edge: PipelineEdge, index: number): Edge {
  return {
    id: `e-${edge.source}-${edge.target}-${index}`,
    source: edge.source,
    target: edge.target,
    label: edge.label ?? undefined,
    animated: false,
  };
}

/** Convert an entire API Pipeline to ReactFlow nodes and edges. */
export function pipelineToRF(pipeline: Pipeline): {
  nodes: Node<DagNodeData>[];
  edges: Edge[];
} {
  const nodes = pipeline.nodes.map(apiNodeToRF);
  const edges = (pipeline.edges ?? []).map(apiEdgeToRF);
  return { nodes, edges };
}

// ── ReactFlow → API ────────────────────────────────────────────────

/** Convert ReactFlow nodes/edges back to an API Pipeline. */
export function rfToPipeline(
  nodes: Node<DagNodeData>[],
  edges: Edge[],
  original: Pipeline,
): Pipeline {
  const pipelineNodes: PipelineNode[] = nodes.map((node) => ({
    id: node.id,
    kind: node.data.kind,
    label: node.data.label,
    config: node.data.config as unknown,
    condition: node.data.condition ?? null,
    position: { x: node.position.x, y: node.position.y },
  }));

  const pipelineEdges: PipelineEdge[] = edges.map((edge) => ({
    source: edge.source,
    target: edge.target,
    label: (typeof edge.label === 'string' ? edge.label : null),
    condition: null,
  }));

  return {
    ...original,
    nodes: pipelineNodes,
    edges: pipelineEdges,
  };
}

// ── Node creation helper ───────────────────────────────────────────

/** Create a new ReactFlow node from a palette drop. */
export function createNode(
  kind: NodeKind,
  position: { x: number; y: number },
): Node<DagNodeData> {
  const id = `${kind}_${Date.now().toString(36)}`;
  return {
    id,
    type: kind,
    position,
    data: {
      kind,
      label: `New ${kind}`,
      config: defaultConfig(kind),
      status: 'idle',
    },
  };
}

// ── YAML export (simple serializer) ────────────────────────────────

/** Serialize a Pipeline to a simple YAML string. */
export function pipelineToYaml(pipeline: Pipeline): string {
  const lines: string[] = [];
  lines.push(`apiVersion: ${pipeline.apiVersion ?? 'goose/v1'}`);
  lines.push(`kind: ${pipeline.kind ?? 'Pipeline'}`);
  lines.push(`name: ${pipeline.name}`);
  if (pipeline.description) {
    lines.push(`description: "${pipeline.description}"`);
  }
  if (pipeline.tags?.length) {
    lines.push(`tags: [${pipeline.tags.join(', ')}]`);
  }
  lines.push('nodes:');

  for (const node of pipeline.nodes) {
    lines.push(`  - id: ${node.id}`);
    lines.push(`    kind: ${node.kind}`);
    lines.push(`    label: "${node.label}"`);
    if (node.condition) {
      lines.push(`    condition: "${node.condition}"`);
    }
    if (node.config != null && typeof node.config === 'object') {
      lines.push('    config:');
      for (const [key, value] of Object.entries(node.config as Record<string, unknown>)) {
        if (value !== undefined && value !== null && value !== '') {
          if (typeof value === 'object') {
            lines.push(`      ${key}:`);
            for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
              lines.push(`        ${k}: "${v}"`);
            }
          } else {
            lines.push(`      ${key}: ${typeof value === 'string' ? `"${value}"` : value}`);
          }
        }
      }
    }
  }

  if (pipeline.edges?.length) {
    lines.push('edges:');
    for (const edge of pipeline.edges) {
      lines.push(`  - source: ${edge.source}`);
      lines.push(`    target: ${edge.target}`);
      if (edge.label) {
        lines.push(`    label: "${edge.label}"`);
      }
    }
  }

  return lines.join('\n');
}

/** Export a pipeline as formatted JSON. */
export function pipelineToJson(pipeline: Pipeline): string {
  return JSON.stringify(pipeline, null, 2);
}
