import { useCallback, useEffect, useId, useMemo, useState } from 'react';
import { Brain, ChevronRight, MessageSquareText } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';
import {
  getPendingToolConfirmationIds,
  getToolRequests,
  getToolResponses,
  type Message,
  type NotificationEvent,
  type ToolRequestMessageContent,
  type ToolResponseMessageContent,
} from '../types/message';
import { cn } from '../utils';
import { getToolCallIcon } from '../utils/toolIconMapping';
import { getToolDescription, getToolName } from '../utils/toolPresentation';
import { ToolIconWithStatus, type ToolCallStatus } from './ToolCallStatusIndicator';
import MarkdownContent from './MarkdownContent';
import ToolCallWithResponse from './ToolCallWithResponse';

const i18n = defineMessages({
  thinking: { id: 'toolTurnSummary.thinking', defaultMessage: 'Thinking' },
  completed: { id: 'toolTurnSummary.completed', defaultMessage: 'Completed in {duration}' },
  failed: { id: 'toolTurnSummary.failed', defaultMessage: 'Failed after {duration}' },
  waitingForApproval: {
    id: 'toolTurnSummary.waitingForApproval',
    defaultMessage: 'Waiting for approval',
  },
  elapsed: { id: 'toolTurnSummary.elapsed', defaultMessage: '{duration} elapsed' },
  seconds: {
    id: 'toolTurnSummary.seconds',
    defaultMessage: '{count, plural, one {# sec} other {# sec}}',
  },
  minutesSeconds: {
    id: 'toolTurnSummary.minutesSeconds',
    defaultMessage:
      '{minutes, plural, one {# min} other {# min}} {seconds, plural, one {# sec} other {# sec}}',
  },
  activityDetails: {
    id: 'toolTurnSummary.activityDetails',
    defaultMessage: 'Agent activity details',
  },
});

type ToolCallValue = { name?: string; arguments?: Record<string, unknown> };
type TurnItem =
  | { kind: 'thinking'; key: string; content: string }
  | { kind: 'message'; key: string; content: string }
  | { kind: 'tool'; key: string; request: ToolRequestMessageContent };

function callOf(request: ToolRequestMessageContent): ToolCallValue | null {
  const call = request.toolCall as { status?: string; value?: ToolCallValue };
  return call.status === 'success' && call.value ? call.value : null;
}

function nameOf(request: ToolRequestMessageContent): string | null {
  const name = callOf(request)?.name;
  return typeof name === 'string' && name ? name : null;
}

function titleOf(request: ToolRequestMessageContent): string | null {
  const metadataTitle = request.metadata?.title;
  if (typeof metadataTitle === 'string' && metadataTitle.trim()) return metadataTitle.trim();
  const call = callOf(request);
  if (!call?.name) return null;
  return getToolDescription(call.name, call.arguments) ?? getToolName(call.name);
}

function responseFailed(response: ToolResponseMessageContent | undefined): boolean {
  const result = response?.toolResult as
    { status?: string; value?: { isError?: boolean } } | undefined;
  return result?.status === 'error' || result?.value?.isError === true;
}

function formatDuration(intl: ReturnType<typeof useIntl>, seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainingSeconds = safeSeconds % 60;
  return minutes > 0
    ? intl.formatMessage(i18n.minutesSeconds, { minutes, seconds: remainingSeconds })
    : intl.formatMessage(i18n.seconds, { count: safeSeconds });
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
  let lastActivityIndex = -1;
  let lastAssistantTextIndex = -1;
  for (let index = blocks.length - 1; index >= 0; index--) {
    const { content, message } = blocks[index];
    if (
      lastAssistantTextIndex === -1 &&
      message.role === 'assistant' &&
      content.type === 'text' &&
      content.text.trim()
    ) {
      lastAssistantTextIndex = index;
    }
    if (
      lastActivityIndex === -1 &&
      (content.type === 'thinking' || content.type === 'toolRequest')
    ) {
      lastActivityIndex = index;
    }
    if (lastActivityIndex !== -1 && lastAssistantTextIndex !== -1) break;
  }
  const finalAssistantTextIndex =
    lastAssistantTextIndex > lastActivityIndex ? lastAssistantTextIndex : -1;

  return blocks.flatMap(
    ({ content, contentIndex, message, messageKey }, blockIndex): TurnItem[] => {
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
        blockIndex !== finalAssistantTextIndex
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
    }
  );
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

export function useToolTurnCollapse(messages: Message[]) {
  const [overrides, setOverrides] = useState<Map<number, boolean>>(new Map());

  const toggleTurn = useCallback((startIndex: number, defaultExpanded = false) => {
    setOverrides((previous) => {
      const next = new Map(previous);
      next.set(startIndex, !(next.get(startIndex) ?? defaultExpanded));
      return next;
    });
  }, []);

  const isTurnExpanded = useCallback(
    (index: number, active = false) => overrides.get(turnStartIndex(messages, index)) ?? active,
    [messages, overrides]
  );

  return { isTurnExpanded, toggleTurn };
}

export function useRunningToolLabel(messages: Message[]): string | undefined {
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
      if (name) return titleOf(requests[j]) ?? getToolName(name);
    }
  }
  return undefined;
}

interface ToolTurnSummaryProps {
  messages: Message[];
  responsesById: Map<string, ToolResponseMessageContent>;
  pendingApprovalIds: Set<string>;
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
  responsesById,
  pendingApprovalIds,
  turnMessages,
  sessionId,
  toolCallNotifications,
  append,
  isExpanded,
  isStreaming,
  onToggle,
}: ToolTurnSummaryProps) {
  const intl = useIntl();
  const detailsId = useId();
  const items = useMemo(
    () => buildTurnItems(turnMessages, messages, pendingApprovalIds),
    [messages, pendingApprovalIds, turnMessages]
  );
  const responses = responsesById;
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!isStreaming) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [isStreaming]);

  if (items.length === 0) return null;

  const timestamped = turnMessages.filter((message) => Number.isFinite(message.created));
  const start =
    (timestamped.find((message) => message.role === 'assistant') ?? timestamped[0])?.created ?? 0;
  const end = timestamped.at(-1)?.created ?? start;
  const duration = formatDuration(intl, isStreaming ? now / 1000 - start : end - start);
  const failed = items.some(
    (item) => item.kind === 'tool' && responseFailed(responses.get(item.request.id))
  );
  const waitingForApproval = turnMessages.some((message) =>
    getToolRequests(message).some((request) => pendingApprovalIds.has(request.id))
  );
  const activeTool = [...items]
    .reverse()
    .find(
      (item): item is Extract<TurnItem, { kind: 'tool' }> =>
        item.kind === 'tool' && !responses.has(item.request.id)
    );
  const summary = waitingForApproval
    ? intl.formatMessage(i18n.waitingForApproval)
    : isStreaming
      ? (activeTool && titleOf(activeTool.request)) || intl.formatMessage(i18n.thinking)
      : intl.formatMessage(failed ? i18n.failed : i18n.completed, { duration });

  return (
    <section
      className="mt-1 flex w-[90%] min-w-0 flex-col"
      aria-label={intl.formatMessage(i18n.activityDetails)}
    >
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={isExpanded}
        aria-controls={detailsId}
        className="group inline-flex min-h-8 max-w-full items-center gap-2 text-left text-sm text-text-secondary transition-colors hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-primary motion-reduce:transition-none"
      >
        <Brain
          className={cn(
            'h-3.5 w-3.5 shrink-0',
            isStreaming && !waitingForApproval && 'animate-pulse motion-reduce:animate-none'
          )}
          aria-hidden="true"
        />
        <span className="min-w-0 truncate">{summary}</span>
        <span aria-hidden="true" className="shrink-0 text-xs tabular-nums">
          {isStreaming && intl.formatMessage(i18n.elapsed, { duration })}
        </span>
        <ChevronRight
          aria-hidden="true"
          className={cn(
            'h-3.5 w-3.5 shrink-0 transition-transform motion-reduce:transition-none rtl:-scale-x-100',
            isExpanded && 'rotate-90'
          )}
        />
      </button>

      {isExpanded && (
        <div
          id={detailsId}
          className="relative ml-[7px] mt-1 border-l border-border-primary/70 pl-5"
          role="region"
        >
          <div className="flex flex-col gap-1.5">
            {items.map((item) => {
              if (item.kind === 'message') {
                return (
                  <div
                    key={item.key}
                    className="flex items-start gap-2 py-1 text-xs text-text-secondary"
                  >
                    <MessageSquareText className="mt-0.5 h-3 w-3 shrink-0" aria-hidden="true" />
                    <div className="min-w-0">
                      <MarkdownContent content={item.content} />
                    </div>
                  </div>
                );
              }

              if (item.kind === 'thinking') {
                return (
                  <details key={item.key} className="group/activity py-1">
                    <summary className="inline-flex max-w-full cursor-pointer list-none items-center gap-2 text-xs text-text-secondary hover:text-text-primary">
                      <Brain className="h-3 w-3 shrink-0" aria-hidden="true" />
                      <span className="min-w-0 truncate">{intl.formatMessage(i18n.thinking)}</span>
                      <ChevronRight
                        className="h-3 w-3 shrink-0 transition-transform group-open/activity:rotate-90 motion-reduce:transition-none rtl:-scale-x-100"
                        aria-hidden="true"
                      />
                    </summary>
                    <div className="ml-5 mt-1 text-xs text-text-secondary">
                      <MarkdownContent content={item.content} />
                    </div>
                  </details>
                );
              }

              const call = callOf(item.request);
              const ToolIcon = getToolCallIcon(call?.name ?? 'tool');
              const response = responses.get(item.request.id);
              const status: ToolCallStatus = responseFailed(response)
                ? 'error'
                : response
                  ? 'success'
                  : isStreaming
                    ? 'loading'
                    : 'pending';

              return (
                <details key={item.key} className="group/activity py-1">
                  <summary className="inline-flex max-w-full cursor-pointer list-none items-center gap-2 text-xs text-text-secondary hover:text-text-primary">
                    <ToolIconWithStatus ToolIcon={ToolIcon} status={status} />
                    <span className="min-w-0 truncate">
                      {titleOf(item.request) ?? intl.formatMessage(i18n.thinking)}
                    </span>
                    <ChevronRight
                      className="h-3 w-3 shrink-0 transition-transform group-open/activity:rotate-90 motion-reduce:transition-none rtl:-scale-x-100"
                      aria-hidden="true"
                    />
                  </summary>
                  <div className="mt-1">
                    <ToolCallWithResponse
                      sessionId={sessionId}
                      isCancelledMessage={false}
                      toolRequest={item.request}
                      toolResponse={response}
                      notifications={toolCallNotifications.get(item.request.id)}
                      isStreamingMessage={isStreaming}
                      isPendingApproval={false}
                      append={append}
                    />
                  </div>
                </details>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}
