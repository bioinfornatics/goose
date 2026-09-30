import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { Brain, ChevronRight, MessageSquareText } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';
import {
  getToolRequests,
  type Message,
  type NotificationEvent,
  type ToolResponseMessageContent,
} from '../types/message';
import { cn } from '../utils';
import { getToolCallIcon } from '../utils/toolIconMapping';
import { buildTurnItems, callOf, titleOf, responseFailed, type TurnItem } from './toolTurnUtils';
import { ToolIconWithStatus, type ToolCallStatus } from './ToolCallStatusIndicator';
import MarkdownContent from './MarkdownContent';
import ToolCallWithResponse from './ToolCallWithResponse';

const i18n = defineMessages({
  incomplete: { id: 'toolTurnSummary.incomplete', defaultMessage: 'Incomplete' },
  thinking: { id: 'toolTurnSummary.thinking', defaultMessage: 'Thinking' },
  completedWithoutDuration: {
    id: 'toolTurnSummary.completedWithoutDuration',
    defaultMessage: 'Completed',
  },
  failedWithoutDuration: { id: 'toolTurnSummary.failedWithoutDuration', defaultMessage: 'Failed' },
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

function formatDuration(intl: ReturnType<typeof useIntl>, seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainingSeconds = safeSeconds % 60;
  return minutes > 0
    ? intl.formatMessage(i18n.minutesSeconds, { minutes, seconds: remainingSeconds })
    : intl.formatMessage(i18n.seconds, { count: safeSeconds });
}

interface LatestThinking {
  content: string;
  key: string;
}

function latestThinkingContent(turnMessages: Message[]): LatestThinking | null {
  for (let messageIndex = turnMessages.length - 1; messageIndex >= 0; messageIndex--) {
    const message = turnMessages[messageIndex];
    if (!message.metadata.userVisible) continue;
    for (let contentIndex = message.content.length - 1; contentIndex >= 0; contentIndex--) {
      const block = message.content[contentIndex];
      if (block.type === 'thinking' && block.thinking.trim()) {
        const messageKey = message.id ?? `turn-${messageIndex}-${message.created}`;
        return {
          content: block.thinking,
          key: `${messageKey}-thinking-${contentIndex}`,
        };
      }
      return null;
    }
  }
  return null;
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
  const latestThinking = useMemo(() => latestThinkingContent(turnMessages), [turnMessages]);
  const [openThinkingKeys, setOpenThinkingKeys] = useState<Set<string>>(new Set());
  const [now, setNow] = useState(() => Date.now());
  const [completedAt, setCompletedAt] = useState<number | null>(null);
  const wasStreaming = useRef(isStreaming);

  useEffect(() => {
    if (!isStreaming) {
      if (wasStreaming.current) setCompletedAt(Date.now());
      wasStreaming.current = false;
      return;
    }
    wasStreaming.current = true;
    setCompletedAt(null);
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [isStreaming]);

  if (items.length === 0) return null;

  const timestamped = turnMessages.filter((message) => Number.isFinite(message.created));
  const start =
    (timestamped.find((message) => message.role === 'assistant') ?? timestamped[0])?.created ?? 0;
  // Message creation times are not completion times. Historical turns without
  // an observed end must not claim a measured duration.
  const end = isStreaming ? now : completedAt;
  const duration = end === null ? null : formatDuration(intl, end / 1000 - start);
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
      : activeTool && !failed
        ? intl.formatMessage(i18n.incomplete)
        : duration === null
          ? intl.formatMessage(failed ? i18n.failedWithoutDuration : i18n.completedWithoutDuration)
          : intl.formatMessage(failed ? i18n.failed : i18n.completed, { duration });
  const showThinkingPreview = isStreaming && !isExpanded && latestThinking !== null;
  const expandLatestThinking = () => {
    if (!latestThinking) return;
    setOpenThinkingKeys((keys) => new Set(keys).add(latestThinking.key));
    onToggle();
  };

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

      {showThinkingPreview && latestThinking && (
        <button
          type="button"
          onClick={expandLatestThinking}
          aria-expanded="false"
          aria-controls={detailsId}
          aria-label={`${intl.formatMessage(i18n.thinking)}: ${latestThinking.content}`}
          className="ml-5 max-w-[min(42rem,calc(100vw-7rem))] cursor-pointer rounded-md px-2 py-1 text-left text-xs text-text-secondary transition-colors hover:bg-background-secondary hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-primary motion-reduce:transition-none"
        >
          <span className="line-clamp-2 break-words">{latestThinking.content}</span>
        </button>
      )}

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
                  <details
                    key={item.key}
                    open={openThinkingKeys.has(item.key)}
                    onToggle={(event) => {
                      const open = event.currentTarget.open;
                      setOpenThinkingKeys((keys) => {
                        const next = new Set(keys);
                        if (open) next.add(item.key);
                        else next.delete(item.key);
                        return next;
                      });
                    }}
                    className="group/activity py-1"
                  >
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
                      hideMcpApp
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
