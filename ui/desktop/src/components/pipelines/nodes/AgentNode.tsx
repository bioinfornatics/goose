import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

export const AgentNode = memo(function AgentNode({ data }: NodeProps) {
  const d = data as Record<string, unknown>;
  const config = d.config as Record<string, unknown> | undefined;

  return (
    <div className="rounded-lg border-2 border-blue-400 bg-white dark:bg-gray-800 px-3 py-2 shadow-sm min-w-[140px]">
      <Handle type="target" position={Position.Top} />
      <div className="text-[10px] text-blue-500 font-semibold uppercase tracking-wide">Agent</div>
      <div className="text-sm font-medium mt-0.5">{String(d.label || 'Agent')}</div>
      {config?.instructions !== undefined && config?.instructions !== null ? (
        <div className="text-xs text-gray-500 mt-1 truncate max-w-[160px]">
          {String(config.instructions)}
        </div>
      ) : null}
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
});
