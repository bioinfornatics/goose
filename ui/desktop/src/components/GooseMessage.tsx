import { useMemo, useRef } from 'react';
import ImagePreview from './ImagePreview';
import { formatMessageTimestamp } from '../utils/timeUtils';
import MarkdownContent from './MarkdownContent';
import ToolCallWithResponse from './ToolCallWithResponse';
import { Brain, ChevronRight } from 'lucide-react';
import {
  getTextAndImageContent,
  getToolRequests,
  getToolResponses,
  getToolConfirmationContent,
  getElicitationContent,
  getPendingToolConfirmationIds,
  getAnyToolConfirmationData,
  MessageWithAttribution,
  ToolConfirmationData,
  NotificationEvent,
} from '../types/message';
import { Message } from '../api';
import ToolCallConfirmation from './ToolCallConfirmation';
import ElicitationRequest from './ElicitationRequest';
import MessageCopyLink from './MessageCopyLink';
import { cn } from '../utils';
import { identifyConsecutiveToolCalls, shouldHideTimestamp } from '../utils/toolCallChaining';
import { useReasoningDetail } from '../contexts/ReasoningDetailContext';

function ThinkingSection({
  cotText,
  isStreaming,
  messageId,
}: {
  cotText: string;
  isStreaming: boolean;
  messageId?: string;
}) {
  const { toggleDetail, isOpen: isPanelOpen, detail } = useReasoningDetail();
  const preview = cotText.split('\n').find((l) => l.trim())?.slice(0, 80) || 'Reasoning...';
  const isThisMessageOpen = isPanelOpen && detail?.messageId === messageId;

  const handleClick = () => {
    if (isStreaming) return;
    toggleDetail({
      title: 'Thought process',
      content: cotText,
      messageId,
    });
  };

  return (
    <div className="mb-2">
      <button
        onClick={handleClick}
        disabled={isStreaming}
        className={cn(
          'flex items-center gap-2 px-3 py-2 rounded-lg border transition-colors select-none group',
          isStreaming
            ? 'bg-background-muted/50 border-border-default/50 cursor-default'
            : 'bg-background-muted/50 border-border-default/50 hover:bg-background-muted cursor-pointer',
          isThisMessageOpen && 'bg-background-muted border-border-default'
        )}
      >
        <Brain
          size={16}
          className={cn(
            'text-text-muted shrink-0',
            isStreaming && 'animate-pulse text-amber-400'
          )}
        />
        <span className="text-sm font-medium text-text-muted">
          {isStreaming ? 'Thinking...' : 'Thought process'}
        </span>
        {!isStreaming && (
          <span className="text-xs text-text-muted/60 truncate text-left max-w-[300px]">
            — {preview}
          </span>
        )}
        {!isStreaming && (
          <ChevronRight
            size={14}
            className={cn(
              'text-text-muted/50 shrink-0 transition-transform duration-200',
              isThisMessageOpen && 'rotate-90'
            )}
          />
        )}
      </button>
    </div>
  );
}

interface GooseMessageProps {
  sessionId: string;
  message: Message;
  messages: Message[];
  metadata?: string[];
  toolCallNotifications: Map<string, NotificationEvent[]>;
  append: (value: string) => void;
  isStreaming: boolean;
  submitElicitationResponse?: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<void>;
}

export default function GooseMessage({
  sessionId,
  message,
  messages,
  toolCallNotifications,
  append,
  isStreaming,
  submitElicitationResponse,
}: GooseMessageProps) {
  const contentRef = useRef<HTMLDivElement | null>(null);

  let { textContent, imagePaths } = getTextAndImageContent(message);

  const stripInternalTags = (text: string): string => {
    // Strip <tool_call>...</tool_call> and <tool_result>...</tool_result> XML tags
    // that some models emit as raw text alongside structured tool calls.
    return text
      .replace(/<tool_call>[\s\S]*?<\/tool_call>/gi, '')
      .replace(/<tool_result>[\s\S]*?<\/tool_result>/gi, '')
      .trim();
  };

  const splitChainOfThought = (text: string): { displayText: string; cotText: string | null } => {
    const regex = /<think>([\s\S]*?)<\/think>/i;
    const match = text.match(regex);
    if (!match) {
      return { displayText: stripInternalTags(text), cotText: null };
    }

    const cotRaw = match[1].trim();
    const displayText = stripInternalTags(text.replace(regex, '').trim());

    return {
      displayText,
      cotText: cotRaw || null,
    };
  };

  const { displayText, cotText } = splitChainOfThought(textContent);

  const timestamp = useMemo(() => formatMessageTimestamp(message.created), [message.created]);
  const modelInfo = (message as MessageWithAttribution)._modelInfo;
  const routingInfo = (message as MessageWithAttribution)._routingInfo;
  const toolRequests = getToolRequests(message);
  const messageIndex = messages.findIndex((msg) => msg.id === message.id);
  const toolConfirmationContent = getToolConfirmationContent(message);
  const elicitationContent = getElicitationContent(message);

  const findConfirmationForToolAcrossMessages = (
    toolRequestId: string
  ): ToolConfirmationData | undefined => {
    for (const msg of messages) {
      const confirmationData = getAnyToolConfirmationData(msg);
      if (confirmationData && confirmationData.id === toolRequestId) {
        return confirmationData;
      }
    }
    return undefined;
  };
  const toolCallChains = useMemo(() => identifyConsecutiveToolCalls(messages), [messages]);
  const hideTimestamp = useMemo(
    () => shouldHideTimestamp(messageIndex, toolCallChains),
    [messageIndex, toolCallChains]
  );
  const hasToolConfirmation = toolConfirmationContent !== undefined;
  const hasElicitation = elicitationContent !== undefined;

  const toolConfirmationShownInline = useMemo(() => {
    if (!toolConfirmationContent) return false;
    const confirmationData = getAnyToolConfirmationData(message);
    if (!confirmationData) return false;

    for (const msg of messages) {
      const requests = getToolRequests(msg);
      if (requests.some((req) => req.id === confirmationData.id)) {
        return true;
      }
    }
    return false;
  }, [toolConfirmationContent, message, messages]);

  const toolResponsesMap = useMemo(() => {
    const responseMap = new Map();

    if (messageIndex !== undefined && messageIndex >= 0) {
      for (let i = messageIndex + 1; i < messages.length; i++) {
        const responses = getToolResponses(messages[i]);

        for (const response of responses) {
          const matchingRequest = toolRequests.find((req) => req.id === response.id);
          if (matchingRequest) {
            responseMap.set(response.id, response);
          }
        }
      }
    }

    return responseMap;
  }, [messages, messageIndex, toolRequests]);

  const pendingConfirmationIds = getPendingToolConfirmationIds(messages);

  return (
    <div className="goose-message flex w-[90%] justify-start min-w-0">
      <div className="flex flex-col w-full min-w-0">
        {cotText && (
          <ThinkingSection
            cotText={cotText}
            isStreaming={isStreaming && !displayText.trim()}
            messageId={message.id ?? undefined}
          />
        )}

        {routingInfo && routingInfo.agentName !== 'Goose Agent' && (
          <div className="flex items-center gap-1.5 mb-1">
            <div className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-blue-500/10 border border-blue-500/20">
              <span className="text-xs font-medium text-blue-400">{routingInfo.agentName}</span>
              <span className="text-xs text-blue-300/70">›</span>
              <span className="text-xs text-blue-300">{routingInfo.modeSlug}</span>
            </div>
          </div>
        )}

        {(displayText.trim() || imagePaths.length > 0) && (
          <div className="flex flex-col group">
            {displayText.trim() && (
              <div ref={contentRef} className="w-full">
                <MarkdownContent content={displayText} />
              </div>
            )}

            {imagePaths.length > 0 && (
              <div className="mt-4">
                {imagePaths.map((imagePath, index) => (
                  <ImagePreview key={index} src={imagePath} />
                ))}
              </div>
            )}

            {toolRequests.length === 0 && (
              <div className="relative flex justify-start">
                {!isStreaming && (
                  <div className="text-xs font-mono text-text-muted pt-1 transition-all duration-200 group-hover:-translate-y-4 group-hover:opacity-0">
                    {timestamp}
                    {routingInfo && (
                      <>
                        <span className="mx-1 opacity-50">·</span>
                        <span className="text-blue-400">{routingInfo.agentName}</span>
                        <span className="mx-1 opacity-50">›</span>
                        <span className="text-blue-300">{routingInfo.modeSlug}</span>
                      </>
                    )}
                    {modelInfo && (
                      <>
                        <span className="mx-1 opacity-50">·</span>
                        <span>{modelInfo.model}</span>
                      </>
                    )}
                  </div>
                )}
                {message.content.every((content) => content.type === 'text') && !isStreaming && (
                  <div className="absolute left-0 pt-1">
                    <MessageCopyLink text={displayText} contentRef={contentRef} />
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {toolRequests.length > 0 && (
          <div className={cn(displayText && 'mt-2')}>
            <div className="relative flex flex-col w-full">
              <div className="flex flex-col gap-3">
                {toolRequests.map((toolRequest) => {
                  const hasResponse = toolResponsesMap.has(toolRequest.id);
                  const isPending = pendingConfirmationIds.has(toolRequest.id);
                  const confirmationContent = findConfirmationForToolAcrossMessages(toolRequest.id);
                  const isApprovalClicked = confirmationContent && !isPending && hasResponse;
                  return (
                    <div className="goose-message-tool" key={toolRequest.id}>
                      <ToolCallWithResponse
                        sessionId={sessionId}
                        isCancelledMessage={false}
                        toolRequest={toolRequest}
                        toolResponse={toolResponsesMap.get(toolRequest.id)}
                        notifications={toolCallNotifications.get(toolRequest.id)}
                        isStreamingMessage={isStreaming}
                        isPendingApproval={isPending}
                        append={append}
                        confirmationContent={confirmationContent}
                        isApprovalClicked={isApprovalClicked}
                      />
                    </div>
                  );
                })}
              </div>
              <div className="text-xs text-text-muted transition-all duration-200 group-hover:-translate-y-4 group-hover:opacity-0 pt-1">
                {!isStreaming && !hideTimestamp && timestamp}
              </div>
            </div>
          </div>
        )}

        {hasToolConfirmation && !toolConfirmationShownInline && (
          <ToolCallConfirmation
            sessionId={sessionId}
            isClicked={false}
            actionRequiredContent={toolConfirmationContent}
          />
        )}

        {hasElicitation && submitElicitationResponse && (
          <ElicitationRequest
            isCancelledMessage={false}
            isClicked={false}
            actionRequiredContent={elicitationContent}
            onSubmit={submitElicitationResponse}
          />
        )}
      </div>
    </div>
  );
}
