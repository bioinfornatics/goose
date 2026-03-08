import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

const kindStyles: Record<string, { border: string; text: string; label: string }> = {
  tool: { border: 'border-purple-400', text: 'text-purple-500', label: 'Tool' },
  transform: { border: 'border-cyan-400', text: 'text-cyan-500', label: 'Transform' },
  human: { border: 'border-orange-400', text: 'text-orange-500', label: 'Human' },
  a2a: { border: 'border-pink-400', text: 'text-pink-500', label: 'A2A' },
};

export const GenericNode = memo(function GenericNode({ data }: NodeProps) {
  const d = data as Record<string, unknown>;
  const kind = String(d.kind || 'tool');
  const style = kindStyles[kind] || kindStyles.tool;

  return (
    <div className={`rounded-lg border-2 ${style.border} bg-white dark:bg-gray-800 px-3 py-2 shadow-sm min-w-[140px]`}>
      <Handle type="target" position={Position.Top} />
      <div className={`text-[10px] ${style.text} font-semibold uppercase tracking-wide`}>
        {style.label}
      </div>
      <div className="text-sm font-medium mt-0.5">{String(d.label || kind)}</div>
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
});
