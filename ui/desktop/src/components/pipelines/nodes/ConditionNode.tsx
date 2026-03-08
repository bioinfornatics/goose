import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

export const ConditionNode = memo(function ConditionNode({ data }: NodeProps) {
  const d = data as Record<string, unknown>;

  return (
    <div className="rounded-lg border-2 border-amber-400 bg-white dark:bg-gray-800 px-3 py-2 shadow-sm min-w-[140px]">
      <Handle type="target" position={Position.Top} />
      <div className="text-[10px] text-amber-500 font-semibold uppercase tracking-wide">Condition</div>
      <div className="text-sm font-medium mt-0.5">{String(d.label || 'Check')}</div>
      <div className="flex justify-between mt-1">
        <Handle
          type="source"
          position={Position.Bottom}
          id="true"
          style={{ left: '30%', background: '#22c55e' }}
        />
        <Handle
          type="source"
          position={Position.Bottom}
          id="false"
          style={{ left: '70%', background: '#ef4444' }}
        />
      </div>
    </div>
  );
});
