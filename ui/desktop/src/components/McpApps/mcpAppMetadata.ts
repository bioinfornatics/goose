import type { ContentBlock, ToolRequestMessageContent } from '../../types/message';

export type UiMeta = {
  ui?: {
    resourceUri?: string;
  };
  extensionName?: string;
  toolName?: string;
  toolNameIsActual?: boolean;
  subagent_session_id?: string;
};

export type ToolResultValue = {
  content: ContentBlock[];
  structuredContent?: unknown;
  isError: boolean;
  _meta?: UiMeta;
};

export type ToolResultWithMeta = {
  status?: string;
  value?: ToolResultValue & {
    _meta?: UiMeta;
  };
};

export type ToolRequestWithMeta = ToolRequestMessageContent & {
  _meta?: UiMeta;
  toolCall: {
    status: 'success';
    value: {
      name: string;
      arguments?: Record<string, unknown>;
    };
  };
};

export function resolveMcpAppMetadata(
  responseMeta: UiMeta | undefined
): { resourceUri: string; extensionName: string; toolName: string } | null {
  const resourceUri = responseMeta?.ui?.resourceUri;
  const extensionName = responseMeta?.extensionName;
  const toolName = responseMeta?.toolName;
  if (resourceUri && extensionName && toolName) {
    const legacyPrefix = `${extensionName}__`;
    const actualToolName = responseMeta.toolNameIsActual
      ? toolName
      : toolName.startsWith(legacyPrefix)
        ? toolName.slice(legacyPrefix.length)
        : toolName;
    if (actualToolName) {
      return { resourceUri, extensionName, toolName: actualToolName };
    }
  }

  return null;
}
