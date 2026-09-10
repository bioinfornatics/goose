import { useCallback, useMemo, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';
import {
  getPendingToolConfirmationIds,
  getToolRequests,
  getToolResponses,
  type Message,
  type NotificationEvent,
  type ToolRequestMessageContent,
} from '../types/message';
import { cn, snakeToTitleCase } from '../utils';
import ThinkingContent from './ThinkingContent';
import ToolCallWithResponse from './ToolCallWithResponse';

const i18n = defineMessages({
  toolCallCount: {
    id: 'toolTurnSummary.toolCallCount',
    // Numbers are spelled out as {count}/{thinking} rather than `#`: extraction
    // flattens the nested plurals, and a `#` then resolves against the inner
    // argument instead of the one it was written for.
    defaultMessage:
      '{count, plural, =0 {Thinking: {thinking}} other {{thinking, plural, =0 {Tool calls: {count}} other {Tool calls: {count} · thinking: {thinking}}}}}',
  },
  running: {
    id: 'toolTurnSummary.running',
    defaultMessage: 'Running {tool}',
  },
});

type ToolCallValue = { name?: string };

function toolLabel(toolCallName: string): string {
  const lastIndex = toolCallName.lastIndexOf('__');
  const shortName = lastIndex === -1 ? toolCallName : toolCallName.substring(lastIndex + 2);
  return snakeToTitleCase(shortName);
}

function nameOf(request: ReturnType<typeof getToolRequests>[number]): string | null {
  const call = request.toolCall as { status?: string; value?: ToolCallValue };
  return call.status === 'success' && typeof call.value?.name === 'string' ? call.value.name : null;
}

type TurnItem =
  | { kind: 'thinking'; key: string; content: string }
  | { kind: 'tool'; key: string; request: ToolRequestMessageContent };

// Calls awaiting approval are rendered in place by GooseMessage, together with
// their buttons. Including them here as well would show each of them twice.
//
// The order comes from the message's own content blocks rather than "thinking
// first, then calls": a single assistant message can carry
// thinking -> toolRequest -> thinking, and the expanded summary should read the
// way the model worked.
export function buildTurnItems(turnMessages: Message[], messages: Message[]): TurnItem[] {
  const pending = getPendingToolConfirmationIds(messages);
  return turnMessages.flatMap((message, messageIndex) => {
    const messageKey = message.id ?? `turn-${messageIndex}-${message.created}`;
    return message.content.flatMap((content, contentIndex): TurnItem[] => {
      if (content.type === 'thinking' && 'thinking' in content && content.thinking) {
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
      return [];
    });
  });
}

// Tool responses arrive as messages with role === 'user'; they must not be
// treated as the start of a turn, or every step would become its own turn.
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

export function useToolTurnCollapse(messages: Message[]) {
  const [expandedTurns, setExpandedTurns] = useState<Set<number>>(new Set());

  const toggleTurn = useCallback((startIndex: number) => {
    setExpandedTurns((previous) => {
      const next = new Set(previous);
      if (!next.delete(startIndex)) next.add(startIndex);
      return next;
    });
  }, []);

  const isTurnExpanded = useCallback(
    (index: number) => expandedTurns.has(turnStartIndex(messages, index)),
    [expandedTurns, messages]
  );

  return { isTurnExpanded, toggleTurn };
}

// The most recent call of the current turn that has no response yet, which is
// the tool actually executing. Once it answers the label goes away and the
// caller falls back to the generic progress message: while the final text is
// being generated no tool is running, and naming one would be a lie.
export function useRunningToolLabel(messages: Message[]): string | undefined {
  const intl = useIntl();

  // Restricted to the current turn: otherwise, right after a request is sent and
  // before any call is made, the line would still show the previous turn's tool.
  const turnStart = turnStartIndex(messages, messages.length - 1);

  const answered = new Set<string>();
  for (let i = turnStart; i < messages.length; i++) {
    for (const response of getToolResponses(messages[i])) answered.add(response.id);
  }

  for (let i = messages.length - 1; i >= turnStart; i--) {
    const requests = getToolRequests(messages[i]);
    for (let j = requests.length - 1; j >= 0; j--) {
      if (answered.has(requests[j].id)) continue;
      const name = nameOf(requests[j]);
      if (name) return intl.formatMessage(i18n.running, { tool: toolLabel(name) });
    }
  }

  return undefined;
}

interface ToolTurnSummaryProps {
  messages: Message[];
  turnMessages: Message[];
  sessionId: string;
  toolCallNotifications: Map<string, NotificationEvent[]>;
  append: (value: string) => void;
  isExpanded: boolean;
  isStreaming: boolean;
  onToggle: () => void;
}

export default function ToolTurnSummary({
  messages,
  turnMessages,
  sessionId,
  toolCallNotifications,
  append,
  isExpanded,
  isStreaming,
  onToggle,
}: ToolTurnSummaryProps) {
  const intl = useIntl();
  const items = useMemo(() => buildTurnItems(turnMessages, messages), [turnMessages, messages]);
  const toolCount = items.filter((item) => item.kind === 'tool').length;
  const thinkingCount = items.length - toolCount;
  const responses = useMemo(() => {
    const map = new Map<string, ReturnType<typeof getToolResponses>[number]>();
    for (const message of messages) {
      for (const response of getToolResponses(message)) map.set(response.id, response);
    }
    return map;
  }, [messages]);

  if (items.length === 0) return null;

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={onToggle}
        className="flex items-center gap-1.5 mt-1 text-xs text-text-secondary hover:text-text-standard transition-colors"
      >
        <ChevronRight
          className={cn(
            'h-3 w-3 shrink-0 transition-transform rtl:-scale-x-100',
            isExpanded && 'rotate-90'
          )}
        />
        <span className="truncate">
          {intl.formatMessage(i18n.toolCallCount, {
            count: toolCount,
            thinking: thinkingCount,
          })}
        </span>
      </button>

      {isExpanded && (
        <div className="flex flex-col gap-3 mt-2">
          {items.map((item) =>
            item.kind === 'thinking' ? (
              // Collapsed by default: an expanded turn is a list of steps, and a
              // thinking block unfolded in full would bury the calls around it.
              <ThinkingContent key={item.key} content={item.content} isExpanded={false} />
            ) : (
              <ToolCallWithResponse
                key={item.key}
                sessionId={sessionId}
                isCancelledMessage={false}
                toolRequest={item.request}
                toolResponse={responses.get(item.request.id)}
                notifications={toolCallNotifications.get(item.request.id)}
                // Without this a call with no response yet counts as successfully finished.
                isStreamingMessage={isStreaming}
                isPendingApproval={false}
                append={append}
              />
            )
          )}
        </div>
      )}
    </div>
  );
}
