import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { ToolRequestMessageContent, ToolResponseMessageContent } from '../../types/message';
import McpAppRenderer from './McpAppRenderer';
import {
  resolveMcpAppMetadata,
  type ToolRequestWithMeta,
  type ToolResultWithMeta,
} from './mcpAppMetadata';

interface McpAppWrapperProps {
  toolRequest: ToolRequestMessageContent;
  toolResponse?: ToolResponseMessageContent;
  sessionId: string;
  append?: (value: string) => void;
}

export default function McpAppWrapper({
  toolRequest,
  toolResponse,
  sessionId,
  append,
}: McpAppWrapperProps): React.ReactNode {
  const requestWithMeta = toolRequest as ToolRequestWithMeta;
  const resultWithMeta = toolResponse?.toolResult as ToolResultWithMeta | undefined;
  const responseMeta =
    resultWithMeta?.status === 'success' && resultWithMeta.value
      ? resultWithMeta.value._meta
      : undefined;
  const appMetadata = resolveMcpAppMetadata(responseMeta);

  const toolArguments =
    requestWithMeta.toolCall.status === 'success'
      ? requestWithMeta.toolCall.value.arguments
      : undefined;

  const toolInput = { arguments: toolArguments || {} };

  const toolResult =
    resultWithMeta?.status === 'success' && resultWithMeta.value
      ? (resultWithMeta.value as unknown as CallToolResult)
      : undefined;

  if (!appMetadata) return null;
  if (requestWithMeta.toolCall.status !== 'success') return null;

  const { resourceUri, extensionName, toolName } = appMetadata;

  return (
    <div className="mt-3">
      <McpAppRenderer
        resourceUri={resourceUri}
        toolInput={toolInput}
        toolResult={toolResult}
        extensionName={extensionName}
        toolName={toolName}
        sessionId={sessionId}
        append={append}
      />
    </div>
  );
}
