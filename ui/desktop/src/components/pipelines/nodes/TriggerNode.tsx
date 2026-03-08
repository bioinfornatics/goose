import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

export const TriggerNode = memo(function TriggerNode({ data }: NodeProps) {
  const d = data as Record<string, unknown>;

  return (
    <div className="rounded-lg border-2 border-green-400 bg-white dark:bg-gray-800 px-3 py-2 shadow-sm min-w-[140px]">
      <div className="text-[10px] text-green-500 font-semibold uppercase tracking-wide">Trigger</div>
      <div className="text-sm font-medium mt-0.5">{String(d.label || 'Start')}</div>
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
});
