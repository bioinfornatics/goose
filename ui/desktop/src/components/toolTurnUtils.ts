import {
  getPendingToolConfirmationIds,
  type Message,
  type ToolRequestMessageContent,
  type ToolResponseMessageContent,
} from '../types/message';
import { getToolDescription, getToolName } from '../utils/toolPresentation';

type ToolCallValue = { name?: string; arguments?: Record<string, unknown> };
export type TurnItem =
  | { kind: 'thinking'; key: string; content: string }
  | { kind: 'message'; key: string; content: string }
  | { kind: 'tool'; key: string; request: ToolRequestMessageContent };

export function callOf(request: ToolRequestMessageContent): ToolCallValue | null {
  const call = request.toolCall as { status?: string; value?: ToolCallValue };
  return call.status === 'success' && call.value ? call.value : null;
}

export function nameOf(request: ToolRequestMessageContent): string | null {
  const name = callOf(request)?.name;
  return typeof name === 'string' && name ? name : null;
}

export function titleOf(request: ToolRequestMessageContent): string | null {
  const metadataTitle = request.metadata?.title;
  if (typeof metadataTitle === 'string' && metadataTitle.trim()) return metadataTitle.trim();
  const call = callOf(request);
  if (!call?.name) return null;
  return getToolDescription(call.name, call.arguments) ?? getToolName(call.name);
}

export function responseFailed(response: ToolResponseMessageContent | undefined): boolean {
  const result = response?.toolResult as
    { status?: string; value?: { isError?: boolean } } | undefined;
  return result?.status === 'error' || result?.value?.isError === true;
}

export function buildTurnItems(
  turnMessages: Message[],
  messages: Message[],
  pending = getPendingToolConfirmationIds(messages)
): TurnItem[] {
  const blocks = turnMessages.flatMap((message, messageIndex) => {
    const messageKey = message.id ?? `turn-${messageIndex}-${message.created}`;
    return message.content.map((content, contentIndex) => ({
      content,
      contentIndex,
      message,
      messageKey,
    }));
  });
  const finalMessage = turnMessages[findTurnFinalMessageIndex(turnMessages)];

  return blocks.flatMap(({ content, contentIndex, message, messageKey }): TurnItem[] => {
    if (!message.metadata.userVisible) return [];
    if (content.type === 'thinking' && content.thinking) {
      return [
        {
          kind: 'thinking',
          key: `${messageKey}-thinking-${contentIndex}`,
          content: content.thinking,
        },
      ];
    }
    if (content.type === 'toolRequest' && !pending.has(content.id)) {
      return [{ kind: 'tool', key: content.id, request: content }];
    }
    if (
      message.role === 'assistant' &&
      content.type === 'text' &&
      content.text.trim() &&
      message !== finalMessage
    ) {
      return [
        {
          kind: 'message',
          key: `${messageKey}-message-${contentIndex}`,
          content: content.text,
        },
      ];
    }
    return [];
  });
}

export function findTurnFinalMessageIndex(messages: Message[]): number {
  let lastImageIndex = -1;
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    if (message.role !== 'assistant' || !message.metadata.userVisible) continue;
    if (
      message.content.some(
        (content) =>
          (content.type === 'text' && content.text.trim()) ||
          content.type === 'toolRequest' ||
          content.type === 'thinking'
      )
    ) {
      return index;
    }
    if (lastImageIndex === -1 && message.content.some((content) => content.type === 'image')) {
      lastImageIndex = index;
    }
  }
  return lastImageIndex;
}

function isRealUserMessage(message: Message): boolean {
  return (
    message.role === 'user' && !message.content.every((content) => content.type === 'toolResponse')
  );
}

export function turnStartIndex(messages: Message[], index: number): number {
  for (let i = index; i >= 0; i--) {
    if (isRealUserMessage(messages[i])) return i;
  }
  return 0;
}

export function turnEndIndex(messages: Message[], index: number): number {
  for (let i = index + 1; i < messages.length; i++) {
    if (isRealUserMessage(messages[i])) return i - 1;
  }
  return messages.length - 1;
}

export function deriveTurnBoundaries(messages: Message[]): {
  startByIndex: number[];
  endByIndex: number[];
} {
  const startByIndex = new Array<number>(messages.length);
  const endByIndex = new Array<number>(messages.length);
  let turnStart = 0;

  for (let index = 0; index < messages.length; index++) {
    if (isRealUserMessage(messages[index])) turnStart = index;
    startByIndex[index] = turnStart;
  }

  let turnEnd = messages.length - 1;
  for (let index = messages.length - 1; index >= 0; index--) {
    if (index + 1 < messages.length && isRealUserMessage(messages[index + 1])) {
      turnEnd = index;
    }
    endByIndex[index] = turnEnd;
  }

  return { startByIndex, endByIndex };
}
