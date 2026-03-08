import { GripVertical } from 'lucide-react';
import type { DragEvent } from 'react';

interface NodeType {
  kind: string;
  label: string;
  description: string;
  color: string;
}

const nodeTypes: NodeType[] = [
  { kind: 'trigger', label: 'Trigger', description: 'Entry point for the pipeline', color: 'bg-green-500' },
  { kind: 'agent', label: 'Agent', description: 'Run an AI agent task', color: 'bg-blue-500' },
  { kind: 'tool', label: 'Tool', description: 'Execute a specific tool', color: 'bg-purple-500' },
  { kind: 'condition', label: 'Condition', description: 'Branch based on logic', color: 'bg-amber-500' },
  { kind: 'transform', label: 'Transform', description: 'Transform data between steps', color: 'bg-cyan-500' },
  { kind: 'human', label: 'Human', description: 'Wait for human input', color: 'bg-orange-500' },
  { kind: 'a2a', label: 'A2A', description: 'Call a remote agent', color: 'bg-pink-500' },
];

export function NodePalette() {
  const onDragStart = (event: DragEvent<HTMLDivElement>, nodeKind: string) => {
    event.dataTransfer.setData('application/reactflow-kind', nodeKind);
    event.dataTransfer.effectAllowed = 'move';
  };

  return (
    <div className="w-56 border-r border-borderSubtle bg-bgApp overflow-y-auto flex-shrink-0">
      <div className="p-3 border-b border-borderSubtle">
        <h2 className="text-sm font-semibold">Nodes</h2>
        <p className="text-xs text-textSubtle mt-0.5">Drag to add to canvas</p>
      </div>
      <div className="p-2 space-y-1">
        {nodeTypes.map((nodeType) => (
          <div
            key={nodeType.kind}
            className="flex items-center gap-2 p-2 rounded-md border border-borderSubtle hover:bg-bgSubtle cursor-grab active:cursor-grabbing transition-colors"
            draggable
            onDragStart={(e) => onDragStart(e, nodeType.kind)}
          >
            <GripVertical className="size-3 text-textSubtle flex-shrink-0" />
            <div className={`w-2.5 h-2.5 rounded-full ${nodeType.color} flex-shrink-0`} />
            <div className="min-w-0">
              <div className="text-sm font-medium">{nodeType.label}</div>
              <div className="text-xs text-textSubtle truncate">{nodeType.description}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
