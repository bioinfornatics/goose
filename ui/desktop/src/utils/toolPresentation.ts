import { snakeToTitleCase } from '../utils';

const MAX_VALUE_LENGTH = 120;

type ToolGraphNode = {
  tool: string;
  description: string;
};

const truncate = (value: string): string =>
  value.length > MAX_VALUE_LENGTH ? `${value.slice(0, MAX_VALUE_LENGTH)}…` : value;

const stringifyValue = (value: unknown): string => {
  if (typeof value === 'string') return value;

  try {
    const serialized = JSON.stringify(value);
    return serialized ?? String(value);
  } catch {
    try {
      return String(value);
    } catch {
      return '[unserializable]';
    }
  }
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const getToolGraph = (value: unknown): ToolGraphNode[] | null => {
  if (!Array.isArray(value) || value.length === 0) return null;

  const nodes = value.filter(
    (node): node is ToolGraphNode =>
      isRecord(node) && typeof node.tool === 'string' && typeof node.description === 'string'
  );
  return nodes.length === value.length ? nodes : null;
};

export const getToolName = (toolCallName: string): string => {
  const lastIndex = toolCallName.lastIndexOf('__');
  return lastIndex === -1 ? toolCallName : toolCallName.substring(lastIndex + 2);
};

export const getToolDescription = (toolCallName: string, toolArguments: unknown): string | null => {
  const args = isRecord(toolArguments) ? toolArguments : {};
  const toolName = getToolName(toolCallName);
  const value = (key: string) => stringifyValue(args[key]);

  switch (toolName) {
    case 'text_editor':
      if (args.command === 'write' && args.path) return `writing ${value('path')}`;
      if (args.command === 'view' && args.path) return `reading ${value('path')}`;
      if (args.command === 'str_replace' && args.path) return `editing ${value('path')}`;
      if (args.command && args.path) return `${truncate(value('command'))} ${value('path')}`;
      break;
    case 'shell':
      if (args.command) return `running ${truncate(value('command'))}`;
      break;
    case 'search':
      if (args.name) return `searching for "${value('name')}"`;
      if (args.mimeType) return `searching for ${value('mimeType')} files`;
      break;
    case 'read':
      if (args.uri) return `reading file ${value('uri').replace('gdrive:///', '')}`;
      if (args.url) return `reading ${value('url')}`;
      break;
    case 'create_file':
      if (args.name) return `creating ${value('name')}`;
      break;
    case 'update_file':
      if (args.fileId) return `updating file ${value('fileId')}`;
      break;
    case 'sheets_tool':
      if (args.operation && args.spreadsheetId)
        return `${value('operation')} in sheet ${value('spreadsheetId')}`;
      break;
    case 'docs_tool':
      if (args.operation && args.documentId)
        return `${value('operation')} in document ${value('documentId')}`;
      break;
    case 'remember_memory':
      if (args.category && args.data) return `storing ${value('category')}: ${value('data')}`;
      break;
    case 'retrieve_memories':
      if (args.category) return `retrieving ${value('category')} memories`;
      break;
    case 'screen_capture':
      return args.window_title ? `capturing window "${value('window_title')}"` : 'capturing screen';
    case 'delegate':
      if (args.instructions) {
        const instructions = value('instructions');
        return `delegating: ${instructions.length > 80 ? `${instructions.slice(0, 80)}…` : instructions}`;
      }
      if (args.source) return `delegating to ${value('source')}`;
      return 'delegating task';
    case 'load':
      return args.source ? `loading ${value('source')}` : 'loading source';
    case 'final_output':
      return 'final output';
    case 'computer_control':
      return 'poking around...';
    case 'execute_typescript': {
      const toolGraph = getToolGraph(args.tool_graph);
      if (!toolGraph) return 'executing code';
      if (toolGraph.length === 1) return truncate(toolGraph[0].description);
      if (toolGraph.length === 2) return `${toolGraph[0].tool}, ${toolGraph[1].tool}`;
      return `${toolGraph.length} tools used`;
    }
    default: {
      const toolDisplayName = snakeToTitleCase(toolName);
      const entries = Object.entries(args);
      if (entries.length === 0) return toolDisplayName;
      if (entries.length === 1) {
        const [key, entryValue] = entries[0];
        return `${toolDisplayName} ${key}: ${truncate(stringifyValue(entryValue))}`;
      }
      return `${toolDisplayName} ${entries.map(([key]) => key).join(', ')}`;
    }
  }

  return null;
};
