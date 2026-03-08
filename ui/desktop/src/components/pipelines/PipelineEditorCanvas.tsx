import { useCallback, useMemo, useRef } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  addEdge,
  useNodesState,
  useEdgesState,
  type Connection,
  type Edge as RFEdge,
  type Node as RFNode,
  type NodeTypes,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import type { Pipeline, PipelineNode, PipelineEdge } from '../../api/types.gen';
import { TriggerNode } from './nodes/TriggerNode';
import { AgentNode } from './nodes/AgentNode';
import { ConditionNode } from './nodes/ConditionNode';
import { GenericNode } from './nodes/GenericNode';

const nodeTypes: NodeTypes = {
  trigger: TriggerNode,
  agent: AgentNode,
  condition: ConditionNode,
  tool: GenericNode,
  transform: GenericNode,
  human: GenericNode,
  a2a: GenericNode,
};

function pipelineNodesToRF(nodes: PipelineNode[]): RFNode[] {
  return nodes.map((n, i) => ({
    id: n.id,
    type: n.kind,
    position: n.position ? { x: n.position.x, y: n.position.y } : { x: 100, y: i * 120 },
    data: {
      label: n.label || n.id,
      kind: n.kind,
      ...(n.config ? { config: n.config } : {}),
      ...(n.condition ? { condition: n.condition } : {}),
    },
  }));
}

function pipelineEdgesToRF(edges: PipelineEdge[]): RFEdge[] {
  return edges.map((e, i) => ({
    id: `e-${e.source}-${e.target}-${i}`,
    source: e.source,
    target: e.target,
    label: e.label || undefined,
    animated: true,
    style: { strokeWidth: 2 },
  }));
}

function rfNodesToPipeline(nodes: RFNode[]): PipelineNode[] {
  return nodes.map((n) => {
    const d = n.data as Record<string, unknown>;
    return {
      id: n.id,
      kind: (n.type || 'agent') as PipelineNode['kind'],
      label: (d.label as string) || n.id,
      config: d.config ?? undefined,
      condition: (d.condition as string) || undefined,
      position: { x: n.position.x, y: n.position.y },
    };
  });
}

function rfEdgesToPipeline(edges: RFEdge[]): PipelineEdge[] {
  return edges.map((e) => ({
    source: e.source,
    target: e.target,
    label: (e.label as string) || undefined,
    condition: undefined,
  }));
}

interface PipelineEditorCanvasProps {
  pipeline: Pipeline;
  onSave: (pipeline: Pipeline) => void;
}

let nodeIdCounter = 0;

export function PipelineEditorCanvas({ pipeline, onSave }: PipelineEditorCanvasProps) {
  const reactFlowWrapper = useRef<HTMLDivElement>(null);

  const initialNodes = useMemo(() => pipelineNodesToRF(pipeline.nodes), [pipeline.nodes]);
  const initialEdges = useMemo(() => pipelineEdgesToRF(pipeline.edges ?? []), [pipeline.edges]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);

  const onConnect = useCallback(
    (connection: Connection) => {
      setEdges((eds) => addEdge({ ...connection, animated: true, style: { strokeWidth: 2 } }, eds));
    },
    [setEdges],
  );

  const handleDragOver = useCallback((event: React.DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  const handleDrop = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      event.preventDefault();
      const kind = event.dataTransfer.getData('application/reactflow-kind');
      if (!kind) return;

      const bounds = reactFlowWrapper.current?.getBoundingClientRect();
      if (!bounds) return;

      const position = {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      };

      const newId = `${kind}-${Date.now()}-${nodeIdCounter++}`;
      const newNode: RFNode = {
        id: newId,
        type: kind,
        position,
        data: { label: kind.charAt(0).toUpperCase() + kind.slice(1), kind },
      };

      setNodes((nds) => [...nds, newNode]);
    },
    [setNodes],
  );

  const handleSave = useCallback(() => {
    const updated: Pipeline = {
      ...pipeline,
      nodes: rfNodesToPipeline(nodes),
      edges: rfEdgesToPipeline(edges),
    };
    onSave(updated);
  }, [pipeline, nodes, edges, onSave]);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === 's') {
        event.preventDefault();
        handleSave();
      }
    },
    [handleSave],
  );

  return (
    <div
      ref={reactFlowWrapper}
      className="flex-1 h-full"
      onKeyDown={onKeyDown}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
      tabIndex={0}
    >
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        nodeTypes={nodeTypes}
        fitView
        deleteKeyCode="Delete"
        className="bg-bgApp"
      >
        <Background gap={16} size={1} />
        <Controls />
        <MiniMap
          nodeStrokeWidth={3}
          pannable
          zoomable
        />
      </ReactFlow>
    </div>
  );
}
