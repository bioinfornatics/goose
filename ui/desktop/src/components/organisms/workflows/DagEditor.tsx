import {
  addEdge,
  Background,
  type Connection,
  Controls,
  type Edge,
  MiniMap,
  type Node,
  type OnConnect,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
} from '@xyflow/react';
import type React from 'react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { client } from '@/api/client.gen';
import '@xyflow/react/dist/style.css';
import {
  FileJson,
  FileText,
  LayoutDashboard,
  Play,
  Redo2,
  Save,
  Square,
  Undo2,
} from 'lucide-react';
import { nodeTypes } from './nodes';
import { NodePalette } from './panels/NodePalette';
import { PropertiesPanel } from './panels/PropertiesPanel';
import type { PipelineTemplate } from './panels/TemplateGallery';
import { createNode, flowToPipeline, pipelineToJson, pipelineToYaml } from './serialization';
import type { DagNodeData, NodeKind, PipelineMetadata } from './types';

interface DagEditorProps {
  initialNodes?: Node<DagNodeData>[];
  initialEdges?: Edge[];
  metadata?: PipelineMetadata;
  onSave?: (yaml: string, json: string) => void;
}

function DagEditorInner({
  initialNodes = [],
  initialEdges = [],
  metadata = { name: 'New Pipeline', description: '' },
  onSave,
}: DagEditorProps) {
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const { screenToFlowPosition } = useReactFlow();

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [pipelineMeta, setPipelineMeta] = useState<PipelineMetadata>(metadata);
  const [showExport, setShowExport] = useState<'yaml' | 'json' | null>(null);
  const [draggingKind, setDraggingKind] = useState<NodeKind | null>(null);

  // History for undo/redo
  const [history, setHistory] = useState<{ nodes: Node<DagNodeData>[]; edges: Edge[] }[]>([]);
  const [historyIdx, setHistoryIdx] = useState(-1);

  const pushHistory = useCallback(() => {
    setHistory((prev) => {
      const next = [...prev.slice(0, historyIdx + 1), { nodes: [...nodes], edges: [...edges] }];
      return next.slice(-50); // Cap at 50 entries
    });
    setHistoryIdx((prev) => Math.min(prev + 1, 49));
  }, [nodes, edges, historyIdx]);

  const undo = useCallback(() => {
    if (historyIdx > 0) {
      const state = history[historyIdx - 1];
      setNodes(state.nodes);
      setEdges(state.edges);
      setHistoryIdx((prev) => prev - 1);
    }
  }, [history, historyIdx, setNodes, setEdges]);

  const redo = useCallback(() => {
    if (historyIdx < history.length - 1) {
      const state = history[historyIdx + 1];
      setNodes(state.nodes);
      setEdges(state.edges);
      setHistoryIdx((prev) => prev + 1);
    }
  }, [history, historyIdx, setNodes, setEdges]);

  // Pipeline execution state
  const [running, setRunning] = useState(false);
  const [runError, setRunError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Update a single node's status
  const updateNodeStatus = useCallback(
    (nodeId: string, status: DagNodeData['status'], output?: string) => {
      setNodes((nds) =>
        nds.map((n) =>
          n.id === nodeId
            ? { ...n, data: { ...n.data, status, ...(output !== undefined && { output }) } }
            : n
        )
      );
    },
    [setNodes]
  );

  // Reset all node statuses
  const resetNodeStatuses = useCallback(() => {
    setNodes((nds) =>
      nds.map((n) => ({ ...n, data: { ...n.data, status: 'idle' as const, output: undefined } }))
    );
  }, [setNodes]);

  // Run pipeline via SSE
  const handleRun = useCallback(async () => {
    if (running) {
      // Stop the current run
      abortRef.current?.abort();
      return;
    }

    // Save first (to ensure pipeline exists on disk)
    const pipeline = flowToPipeline(nodes, edges, pipelineMeta);
    const yaml = pipelineToYaml(pipeline);
    const json = pipelineToJson(pipeline);
    onSave?.(yaml, json);

    setRunning(true);
    setRunError(null);
    resetNodeStatuses();

    const abort = new AbortController();
    abortRef.current = abort;

    try {
      const baseUrl = client.getConfig().baseUrl || '';
      const headers: Record<string, string> = { 'Content-Type': 'application/json' };
      const rawHeaders = client.getConfig().headers;
      if (rawHeaders) {
        const h = rawHeaders as Record<string, string>;
        const secretKey =
          typeof h.get === 'function'
            ? (h as unknown as globalThis.Headers).get('X-Secret-Key')
            : h['X-Secret-Key'];
        if (secretKey) {
          headers['X-Secret-Key'] = secretKey;
        }
      }

      const resp = await fetch(
        `${baseUrl}/pipelines/${encodeURIComponent(pipelineMeta.name)}/run`,
        {
          method: 'POST',
          headers,
          body: JSON.stringify({ max_concurrency: 4 }),
          signal: abort.signal,
        }
      );

      if (!resp.ok) {
        const text = await resp.text();
        throw new Error(`Pipeline execution failed: ${resp.status} ${text}`);
      }

      // Read SSE stream
      const reader = resp.body?.getReader();
      const decoder = new TextDecoder();
      if (!reader) throw new Error('No response body');

      let buffer = '';
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.startsWith('data:')) continue;
          const jsonStr = line.slice(5).trim();
          if (!jsonStr || jsonStr === '[DONE]') continue;

          try {
            const event = JSON.parse(jsonStr);
            switch (event.event) {
              case 'node_started':
                updateNodeStatus(event.node_id, 'running');
                break;
              case 'node_completed':
                updateNodeStatus(event.node_id, 'success', event.output);
                break;
              case 'node_failed':
                updateNodeStatus(event.node_id, 'error', event.error);
                break;
              case 'node_skipped':
                updateNodeStatus(event.node_id, 'skipped');
                break;
              case 'run_completed':
                break;
              case 'run_failed':
                setRunError(event.error || 'Pipeline execution failed');
                break;
            }
          } catch {
            // Skip malformed JSON lines
          }
        }
      }
    } catch (err: unknown) {
      if (err instanceof Error && err.name !== 'AbortError') {
        setRunError(err.message);
      }
    } finally {
      setRunning(false);
      abortRef.current = null;
    }
  }, [running, nodes, edges, pipelineMeta, onSave, resetNodeStatuses, updateNodeStatus]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  // Load template into canvas
  const handleTemplateSelect = useCallback(
    (template: PipelineTemplate) => {
      pushHistory();
      const { nodes: tplNodes, edges: tplEdges, metadata: tplMeta } = template.build();
      setNodes(tplNodes);
      setEdges(tplEdges);
      setPipelineMeta(tplMeta);
    },
    [pushHistory, setNodes, setEdges]
  );

  // Auto-layout (simple layered DAG layout)
  const autoLayout = useCallback(() => {
    pushHistory();
    const inDegree = new Map<string, number>();
    const children = new Map<string, string[]>();
    for (const n of nodes) {
      inDegree.set(n.id, 0);
      children.set(n.id, []);
    }
    for (const e of edges) {
      inDegree.set(e.target, (inDegree.get(e.target) || 0) + 1);
      children.get(e.source)?.push(e.target);
    }

    // Topological sort into layers
    const layers: string[][] = [];
    const placed = new Set<string>();
    let remaining = [...nodes.map((n) => n.id)];

    while (remaining.length > 0) {
      const layer = remaining.filter((id) => (inDegree.get(id) || 0) === 0 && !placed.has(id));
      if (layer.length === 0) {
        // Cycle fallback — place remaining in one layer
        layers.push(remaining);
        break;
      }
      layers.push(layer);
      for (const id of layer) {
        placed.add(id);
        for (const child of children.get(id) || []) {
          inDegree.set(child, (inDegree.get(child) || 0) - 1);
        }
      }
      remaining = remaining.filter((id) => !placed.has(id));
    }

    const X_GAP = 280;
    const Y_GAP = 120;
    const updated = nodes.map((n) => {
      const layerIdx = layers.findIndex((l) => l.includes(n.id));
      const posInLayer = layers[layerIdx]?.indexOf(n.id) ?? 0;
      const layerSize = layers[layerIdx]?.length ?? 1;
      const yOffset = -(layerSize - 1) * Y_GAP * 0.5;
      return {
        ...n,
        position: { x: layerIdx * X_GAP, y: yOffset + posInLayer * Y_GAP },
      };
    });
    setNodes(updated);
  }, [nodes, edges, pushHistory, setNodes]);

  // Edge validation — prevent self-loops and duplicate edges
  const isValidConnection = useCallback(
    (connection: Connection | Edge) => {
      if (connection.source === connection.target) return false;
      const exists = edges.some(
        (e) => e.source === connection.source && e.target === connection.target
      );
      return !exists;
    },
    [edges]
  );

  // Connect nodes
  const onConnect: OnConnect = useCallback(
    (params: Connection) => {
      pushHistory();
      setEdges((eds) =>
        addEdge(
          {
            ...params,
            animated: false,
            style: { strokeWidth: 2, stroke: '#6366f1' },
          },
          eds
        )
      );
    },
    [setEdges, pushHistory]
  );

  // Drop node from palette
  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();
      const kind = event.dataTransfer.getData('application/dagnode') as NodeKind;
      if (!kind) return;

      const position = screenToFlowPosition({
        x: event.clientX,
        y: event.clientY,
      });

      pushHistory();
      const newNode = createNode(kind, position);
      setNodes((nds) => [...nds, newNode]);
      setSelectedNodeId(newNode.id);
      setDraggingKind(null);
    },
    [screenToFlowPosition, setNodes, pushHistory]
  );

  // Node selection
  const onNodeClick = useCallback((_: React.MouseEvent, node: Node) => {
    setSelectedNodeId(node.id);
  }, []);

  const onPaneClick = useCallback(() => {
    setSelectedNodeId(null);
  }, []);

  // Update node data from properties panel
  const onUpdateNode = useCallback(
    (nodeId: string, data: Partial<DagNodeData>) => {
      pushHistory();
      setNodes((nds) =>
        nds.map((n) => (n.id === nodeId ? { ...n, data: { ...n.data, ...data } } : n))
      );
    },
    [setNodes, pushHistory]
  );

  // Delete node
  const onDeleteNode = useCallback(
    (nodeId: string) => {
      pushHistory();
      setNodes((nds) => nds.filter((n) => n.id !== nodeId));
      setEdges((eds) => eds.filter((e) => e.source !== nodeId && e.target !== nodeId));
      setSelectedNodeId(null);
    },
    [setNodes, setEdges, pushHistory]
  );

  // Selected node data
  const selectedNode = useMemo(
    () => nodes.find((n) => n.id === selectedNodeId),
    [nodes, selectedNodeId]
  );

  // Save / Export
  const handleSave = useCallback(() => {
    const pipeline = flowToPipeline(nodes, edges, pipelineMeta);
    const yaml = pipelineToYaml(pipeline);
    const json = pipelineToJson(pipeline);
    onSave?.(yaml, json);
  }, [nodes, edges, pipelineMeta, onSave]);

  const handleExport = useCallback(
    (format: 'yaml' | 'json') => {
      const pipeline = flowToPipeline(nodes, edges, pipelineMeta);
      const content = format === 'yaml' ? pipelineToYaml(pipeline) : pipelineToJson(pipeline);
      navigator.clipboard.writeText(content);
      setShowExport(format);
      setTimeout(() => setShowExport(null), 2000);
    },
    [nodes, edges, pipelineMeta]
  );

  // Keyboard shortcuts (Delete, Ctrl+Z, Ctrl+Y, Ctrl+S)
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const isInput =
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement;

      if (e.key === 'Delete' || e.key === 'Backspace') {
        if (!isInput && selectedNodeId) {
          e.preventDefault();
          onDeleteNode(selectedNodeId);
        }
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
        e.preventDefault();
        undo();
      }
      if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) {
        e.preventDefault();
        redo();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        handleSave();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [selectedNodeId, onDeleteNode, undo, redo, handleSave]);

  return (
    <div className="flex h-full w-full bg-background-default">
      {/* Node Palette */}
      <NodePalette onDragStart={setDraggingKind} onTemplateSelect={handleTemplateSelect} />

      {/* Canvas */}
      <div className="flex-1 relative" ref={reactFlowWrapper}>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          isValidConnection={isValidConnection}
          onDragOver={onDragOver}
          onDrop={onDrop}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          nodeTypes={nodeTypes}
          defaultEdgeOptions={{ type: 'smoothstep', style: { strokeWidth: 2, stroke: '#6366f1' } }}
          fitView
          snapToGrid
          snapGrid={[20, 20]}
          className={draggingKind ? 'cursor-copy' : ''}
          proOptions={{ hideAttribution: true }}
        >
          <Background gap={20} size={1} />
          <Controls
            className="!bg-background-default !border-border-default !shadow-md"
            showInteractive={false}
          />
          <MiniMap
            className="!bg-background-muted !border-border-default"
            nodeColor={(node) => {
              const data = node.data as DagNodeData;
              const colors: Record<NodeKind, string> = {
                trigger: '#6366f1',
                agent: '#8b5cf6',
                tool: '#0ea5e9',
                condition: '#f59e0b',
                transform: '#10b981',
                human: '#ec4899',
                a2a: '#14b8a6',
              };
              return colors[data.kind] ?? '#6b7280';
            }}
          />

          {/* Toolbar */}
          <Panel position="top-center">
            <div className="flex items-center gap-1 bg-background-default border border-border-default rounded-lg shadow-md px-2 py-1">
              {/* Pipeline name */}
              <input
                type="text"
                value={pipelineMeta.name}
                onChange={(e) => setPipelineMeta((m) => ({ ...m, name: e.target.value }))}
                className="text-sm font-medium text-text-default bg-transparent border-none
                           focus:outline-none focus:ring-0 w-40 text-center"
                placeholder="Pipeline name"
              />

              <div className="w-px h-5 bg-border-muted mx-1" />

              <button
                type="button"
                onClick={undo}
                disabled={historyIdx <= 0}
                className="p-1.5 rounded-md hover:bg-background-muted disabled:opacity-30 text-text-muted"
                title="Undo"
              >
                <Undo2 size={14} />
              </button>
              <button
                type="button"
                onClick={redo}
                disabled={historyIdx >= history.length - 1}
                className="p-1.5 rounded-md hover:bg-background-muted disabled:opacity-30 text-text-muted"
                title="Redo (Ctrl+Y)"
              >
                <Redo2 size={14} />
              </button>
              <button
                type="button"
                onClick={autoLayout}
                disabled={nodes.length === 0}
                className="p-1.5 rounded-md hover:bg-background-muted disabled:opacity-30 text-text-muted"
                title="Auto-layout"
              >
                <LayoutDashboard size={14} />
              </button>

              <div className="w-px h-5 bg-border-muted mx-1" />

              <button
                type="button"
                onClick={() => handleExport('yaml')}
                className="p-1.5 rounded-md hover:bg-background-muted text-text-muted"
                title="Copy as YAML"
              >
                <FileText size={14} />
              </button>
              <button
                type="button"
                onClick={() => handleExport('json')}
                className="p-1.5 rounded-md hover:bg-background-muted text-text-muted"
                title="Copy as JSON"
              >
                <FileJson size={14} />
              </button>

              <div className="w-px h-5 bg-border-muted mx-1" />

              <button
                type="button"
                onClick={handleSave}
                className="flex items-center gap-1 px-2 py-1 rounded-md
                           bg-background-accent text-text-on-accent text-xs font-medium
                           hover:opacity-90 transition-opacity"
              >
                <Save size={12} />
                Save
              </button>

              <div className="w-px h-5 bg-border-muted mx-1" />

              {/* Run / Stop */}
              <button
                type="button"
                onClick={handleRun}
                disabled={nodes.length === 0}
                className={`flex items-center gap-1 px-2 py-1 rounded-md text-xs font-medium
                           transition-all disabled:opacity-30 ${
                             running
                               ? 'bg-red-600 text-white hover:bg-red-700'
                               : 'bg-emerald-600 text-white hover:bg-emerald-700'
                           }`}
                title={running ? 'Stop execution' : 'Run pipeline'}
              >
                {running ? (
                  <>
                    <Square size={12} />
                    Stop
                  </>
                ) : (
                  <>
                    <Play size={12} />
                    Run
                  </>
                )}
              </button>

              {showExport && (
                <span className="text-xs text-text-success ml-1">
                  Copied {showExport.toUpperCase()}!
                </span>
              )}
            </div>
          </Panel>

          {/* Run error banner */}
          {runError && (
            <Panel position="top-center" className="mt-14">
              <div className="flex items-center gap-2 px-3 py-1.5 bg-red-100 dark:bg-red-900/30 border border-red-300 dark:border-red-700 rounded-lg text-xs text-red-700 dark:text-red-300">
                <span>{runError}</span>
                <button
                  type="button"
                  onClick={() => setRunError(null)}
                  className="ml-1 font-bold hover:opacity-70"
                >
                  ×
                </button>
              </div>
            </Panel>
          )}

          {/* Empty state */}
          {nodes.length === 0 && (
            <Panel position="top-center" className="mt-20">
              <div className="text-center text-text-muted">
                <p className="text-sm font-medium">Drag nodes from the palette</p>
                <p className="text-xs mt-1">Start with a Trigger node</p>
              </div>
            </Panel>
          )}
        </ReactFlow>
      </div>

      {/* Properties Panel */}
      {selectedNode && (
        <PropertiesPanel
          nodeId={selectedNode.id}
          data={selectedNode.data as DagNodeData}
          onUpdate={onUpdateNode}
          onDelete={onDeleteNode}
          onClose={() => setSelectedNodeId(null)}
        />
      )}
    </div>
  );
}

export function DagEditor(props: DagEditorProps) {
  return (
    <ReactFlowProvider>
      <DagEditorInner {...props} />
    </ReactFlowProvider>
  );
}
