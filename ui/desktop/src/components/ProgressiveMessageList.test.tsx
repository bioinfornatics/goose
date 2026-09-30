import { StrictMode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import type { ImageData, Message, MessageContent } from '../types/message';
import { IntlTestWrapper } from '../i18n/test-utils';
import ProgressiveMessageList from './ProgressiveMessageList';

const renderCounts = vi.hoisted(() => new Map<string, number>());
const messageUpdateCallbacks = vi.hoisted(
  () =>
    new Map<
      string,
      | ((
          messageId: string,
          newContent: string,
          editType: 'fork' | 'edit',
          retainedImages: ImageData[]
        ) => void)
      | undefined
    >()
);

const turnFinalByMessageId = vi.hoisted(() => new Map<string, boolean | undefined>());
const streamingByMessageId = vi.hoisted(() => new Map<string, boolean>());

vi.mock('./GooseMessage', () => ({
  default: ({
    message,
    isTurnFinal,
    isStreaming,
    collapseToolCalls,
    toolStates,
  }: {
    message: Message;
    isTurnFinal?: boolean;
    isStreaming: boolean;
    collapseToolCalls?: boolean;
    toolStates: { requestId: string; isPending: boolean }[];
  }) => {
    const id = message.id ?? 'missing-id';
    renderCounts.set(id, (renderCounts.get(id) ?? 0) + 1);
    turnFinalByMessageId.set(id, isTurnFinal);
    streamingByMessageId.set(id, isStreaming);
    return (
      <div>
        {id}
        {message.content.map((content, index) => {
          if (content.type === 'text' && (!collapseToolCalls || isTurnFinal)) {
            return <span key={index}>{content.text}</span>;
          }
          if (content.type === 'thinking' && !collapseToolCalls) {
            return <span key={index}>inline-thinking:{content.thinking}</span>;
          }
          if (content.type === 'toolRequest') {
            const state = toolStates.find((candidate) => candidate.requestId === content.id);
            if (!collapseToolCalls || state?.isPending) {
              return <span key={index}>inline-tool:{content.id}</span>;
            }
          }
          return null;
        })}
      </div>
    );
  },
}));

vi.mock('./ThinkingContent', () => ({
  default: ({ content }: { content: string }) => <div>summary-thinking:{content}</div>,
}));

vi.mock('./ToolCallWithResponse', () => ({
  default: ({
    toolRequest,
    isStreamingMessage,
  }: {
    toolRequest: { id: string };
    isStreamingMessage: boolean;
  }) => <div>{`summary-tool:${toolRequest.id}:streaming:${isStreamingMessage}`}</div>,
}));

vi.mock('./UserMessage', () => ({
  default: ({
    message,
    onMessageUpdate,
  }: {
    message: Message;
    onMessageUpdate?: (
      messageId: string,
      newContent: string,
      editType: 'fork' | 'edit',
      retainedImages: ImageData[]
    ) => void;
  }) => {
    const id = message.id ?? 'missing-id';
    renderCounts.set(id, (renderCounts.get(id) ?? 0) + 1);
    messageUpdateCallbacks.set(id, onMessageUpdate);
    return <div>{id}</div>;
  },
}));

const visibleMetadata: Message['metadata'] = { agentVisible: true, userVisible: true };
const append = vi.fn();
const isUserMessage = (message: Message) => message.role === 'user';

function message(id: string, role: Message['role'], content: MessageContent[]): Message {
  return { id, role, created: 1, content, metadata: visibleMetadata };
}

function cloneMessages(messages: Message[]): Message[] {
  return messages.map((item) => ({
    ...item,
    content: item.content.map((content) => ({ ...content })),
    metadata: { ...item.metadata },
  }));
}

function toolRequest(id: string): MessageContent {
  return {
    type: 'toolRequest',
    id,
    toolCall: {
      status: 'success',
      value: { name: 'test_tool', arguments: {} },
    },
  };
}

function toolResponse(id: string): MessageContent {
  return {
    type: 'toolResponse',
    id,
    toolResult: {
      status: 'success',
      value: { content: [{ type: 'text', text: 'complete' }], isError: false },
    },
  };
}

function renderList(messages: Message[]) {
  return render(
    <ProgressiveMessageList
      messages={messages}
      sessionId="test-session"
      append={append}
      isUserMessage={isUserMessage}
    />,
    { wrapper: IntlTestWrapper }
  );
}

describe('ProgressiveMessageList render isolation', () => {
  beforeEach(() => {
    renderCounts.clear();
    messageUpdateCallbacks.clear();
    append.mockClear();
  });

  it('does not rerender historical rows from cloned equivalent messages', () => {
    const messages = [
      message('assistant-1', 'assistant', [{ type: 'text', text: 'First' }]),
      message('user-1', 'user', [{ type: 'text', text: 'Continue' }]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'Streaming' }]),
    ];
    const { rerender } = renderList(messages);

    rerender(
      <ProgressiveMessageList
        messages={cloneMessages(messages)}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
      />
    );

    expect(renderCounts).toEqual(
      new Map([
        ['assistant-1', 1],
        ['user-1', 1],
        ['assistant-2', 1],
      ])
    );

    const updatedMessages = cloneMessages(messages);
    updatedMessages[2].content = [{ type: 'text', text: 'Streaming update' }];
    rerender(
      <ProgressiveMessageList
        messages={updatedMessages}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
      />
    );

    expect(renderCounts).toEqual(
      new Map([
        ['assistant-1', 1],
        ['user-1', 1],
        ['assistant-2', 2],
      ])
    );
  });

  it('rerenders the matching request row when a tool response arrives', () => {
    const messages = [
      message('tool-request', 'assistant', [toolRequest('tool-1')]),
      message('unrelated', 'assistant', [{ type: 'text', text: 'Unrelated' }]),
      message('tool-response', 'user', []),
    ];
    const { rerender } = renderList(messages);
    const updatedMessages = cloneMessages(messages);
    updatedMessages[2].content = [toolResponse('tool-1')];

    rerender(
      <ProgressiveMessageList
        messages={updatedMessages}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
      />
    );

    expect(renderCounts.get('tool-request')).toBe(2);
    expect(renderCounts.get('unrelated')).toBe(1);
  });

  it('preserves the message update callback', () => {
    const onMessageUpdate = vi.fn();
    render(
      <ProgressiveMessageList
        messages={[message('user-1', 'user', [{ type: 'text', text: 'Original' }])]}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
        onMessageUpdate={onMessageUpdate}
      />,
      { wrapper: IntlTestWrapper }
    );
    const retainedImages: ImageData[] = [{ data: 'image', mimeType: 'image/png' }];

    messageUpdateCallbacks.get('user-1')?.('user-1', 'Updated', 'fork', retainedImages);

    expect(onMessageUpdate).toHaveBeenCalledWith('user-1', 'Updated', 'fork', retainedImages);
  });
});

describe('ProgressiveMessageList batching', () => {
  const messages = Array.from({ length: 10 }, (_, index) =>
    message(`assistant-${index}`, 'assistant', [{ type: 'text', text: `Message ${index}` }])
  );

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function renderBatchedList(onRenderingComplete = vi.fn()) {
    render(
      <StrictMode>
        <IntlTestWrapper>
          <ProgressiveMessageList
            messages={messages}
            sessionId="test-session"
            append={append}
            isUserMessage={isUserMessage}
            batchSize={2}
            batchDelay={20}
            showLoadingThreshold={0}
            onRenderingComplete={onRenderingComplete}
          />
        </IntlTestWrapper>
      </StrictMode>
    );
    return onRenderingComplete;
  }

  it('renders exactly one batch per delay in StrictMode', () => {
    renderBatchedList();

    expect(screen.queryByText('assistant-1')).not.toBeNull();
    expect(screen.queryByText('assistant-2')).toBeNull();

    act(() => vi.advanceTimersByTime(20));
    expect(screen.queryByText('assistant-3')).not.toBeNull();
    expect(screen.queryByText('assistant-4')).toBeNull();

    act(() => vi.advanceTimersByTime(20));
    expect(screen.queryByText('assistant-5')).not.toBeNull();
    expect(screen.queryByText('assistant-6')).toBeNull();
  });

  it('reports completion once after the final batch', () => {
    const onRenderingComplete = renderBatchedList();

    for (let batch = 0; batch < 4; batch++) {
      act(() => vi.advanceTimersByTime(20));
    }
    expect(screen.queryByText('assistant-9')).not.toBeNull();
    expect(onRenderingComplete).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(50));
    expect(onRenderingComplete).toHaveBeenCalledTimes(1);
  });
});

// Tool calls are collapsed per turn, see ToolTurnSummary.
// A turn runs from one real user message to the next; spacing asks whether an
// earlier reply of the turn was rendered, the timestamp/usage footer asks
// whether this reply ends the turn.
describe('ProgressiveMessageList turn grouping', () => {
  beforeEach(() => {
    renderCounts.clear();
    turnFinalByMessageId.clear();
    streamingByMessageId.clear();
    append.mockClear();
  });

  function containerClasses(): string[] {
    return screen.getAllByTestId('message-container').map((row) => row.className);
  }

  it('does not treat the first reply of a turn as a continuation', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'Do X' }]),
      message('assistant-1', 'assistant', [{ type: 'text', text: 'Done' }]),
    ]);

    const [, firstReply] = containerClasses();
    expect(firstReply).toContain('mt-4');
    expect(firstReply).not.toContain('mt-1');
    expect(turnFinalByMessageId.get('assistant-1')).toBe(true);
  });

  it('keeps intermediate replies tight and gives the footer to the last one', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'Do X' }]),
      message('assistant-1', 'assistant', [{ type: 'text', text: 'Let me check' }]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'Done' }]),
    ]);

    const [, firstReply, secondReply] = containerClasses();
    expect(firstReply).toContain('mt-4');
    expect(secondReply).toContain('mt-1');
    expect(turnFinalByMessageId.get('assistant-1')).toBe(false);
    expect(turnFinalByMessageId.get('assistant-2')).toBe(true);
  });

  it('starts a new turn on the next user message', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'First question' }]),
      message('assistant-1', 'assistant', [{ type: 'text', text: 'First answer' }]),
      message('user-2', 'user', [{ type: 'text', text: 'Second question' }]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'Second answer' }]),
    ]);

    const [, , secondQuestion, secondReply] = containerClasses();
    expect(secondQuestion).toContain('mt-4');
    expect(secondReply).toContain('mt-4');
    expect(turnFinalByMessageId.get('assistant-1')).toBe(true);
    expect(turnFinalByMessageId.get('assistant-2')).toBe(true);
  });

  it('does not end a turn on a tool response carrying the user role', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'Do X' }]),
      message('assistant-1', 'assistant', [toolRequest('call-1')]),
      message('tool-response-1', 'user', [toolResponse('call-1')]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'Done' }]),
    ]);

    expect(turnFinalByMessageId.get('assistant-1')).toBe(false);
    expect(turnFinalByMessageId.get('assistant-2')).toBe(true);
  });

  it('renders exactly one summary for a multi-step turn without duplicating source content', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'Investigate' }]),
      message('assistant-1', 'assistant', [
        { type: 'thinking', thinking: 'inspect logs', signature: 'sig' },
        toolRequest('call-1'),
      ]),
      message('tool-response-1', 'user', [toolResponse('call-1')]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'Final assistant response' }]),
    ]);

    expect(screen.getAllByText('Completed')).toHaveLength(1);
    expect(screen.queryByText('inline-thinking:inspect logs')).toBeNull();
    expect(screen.queryByText('inline-tool:call-1')).toBeNull();
    expect(screen.queryByText('summary-thinking:inspect logs')).toBeNull();
    expect(screen.queryByText('summary-tool:call-1:streaming:false')).toBeNull();
    expect(screen.getByText('Final assistant response')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Completed' }));
    expect(screen.getByText('Thinking')).toBeTruthy();
    expect(screen.getByText('inspect logs')).toBeTruthy();
    expect(screen.getAllByText('summary-tool:call-1:streaming:false')).toHaveLength(1);
  });

  it('keeps a pending approval out of the summary', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'Delete it' }]),
      message('assistant-1', 'assistant', [toolRequest('approval-call')]),
      message('approval-1', 'assistant', [
        {
          type: 'actionRequired',
          data: {
            actionType: 'toolConfirmation',
            id: 'approval-call',
            toolName: 'test_tool',
            arguments: {},
          },
        },
      ]),
    ]);

    expect(screen.getByText('inline-tool:approval-call')).toBeTruthy();
    expect(screen.queryByRole('region', { name: 'Agent activity details' })).toBeNull();
  });

  it('creates separate summaries for separate user turns', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'First' }]),
      message('assistant-1', 'assistant', [toolRequest('call-1')]),
      message('response-1', 'user', [toolResponse('call-1')]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'First done' }]),
      message('user-2', 'user', [{ type: 'text', text: 'Second' }]),
      message('assistant-3', 'assistant', [toolRequest('call-2')]),
      message('response-2', 'user', [toolResponse('call-2')]),
      message('assistant-4', 'assistant', [{ type: 'text', text: 'Second done' }]),
    ]);

    expect(screen.getAllByText('Completed')).toHaveLength(2);
  });

  it('applies streaming only to the active last turn and preserves historical rows', () => {
    const messages = [
      message('user-1', 'user', [{ type: 'text', text: 'First' }]),
      message('assistant-1', 'assistant', [toolRequest('call-1')]),
      message('response-1', 'user', [toolResponse('call-1')]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'First done' }]),
      message('user-2', 'user', [{ type: 'text', text: 'Second' }]),
      message('assistant-3', 'assistant', [toolRequest('call-2')]),
    ];
    const { rerender } = render(
      <ProgressiveMessageList
        messages={messages}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
        isStreamingMessage
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(streamingByMessageId.get('assistant-1')).toBe(false);
    expect(streamingByMessageId.get('assistant-3')).toBe(true);
    const historicalRenderCount = renderCounts.get('assistant-1');

    rerender(
      <ProgressiveMessageList
        messages={cloneMessages(messages)}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
        isStreamingMessage
      />
    );

    expect(renderCounts.get('assistant-1')).toBe(historicalRenderCount);
    const summaries = screen.getAllByRole('button', { name: /Completed|Test Tool/ });
    fireEvent.click(summaries[0]);
    expect(summaries[1]).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('summary-tool:call-1:streaming:false')).toBeTruthy();
    expect(screen.getByText('summary-tool:call-2:streaming:true')).toBeTruthy();
  });

  it('summarises a call made after the turn opened with a plain reply', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'Do X' }]),
      message('assistant-1', 'assistant', [{ type: 'text', text: 'Let me check' }]),
      message('assistant-2', 'assistant', [toolRequest('call-1')]),
      message('tool-response-1', 'user', [toolResponse('call-1')]),
      message('assistant-3', 'assistant', [{ type: 'text', text: 'Done' }]),
    ]);

    expect(screen.getByText('Completed')).toBeTruthy();
  });

  it('summarises a call made after a status notification opened the turn', () => {
    renderList([
      message('user-1', 'user', [{ type: 'text', text: 'Do X' }]),
      message('status-1', 'assistant', [
        { type: 'systemNotification', notificationType: 'inlineMessage', msg: 'Compacting' },
      ]),
      message('assistant-1', 'assistant', [toolRequest('call-1')]),
      message('tool-response-1', 'user', [toolResponse('call-1')]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'Done' }]),
    ]);

    expect(screen.getByText('Completed')).toBeTruthy();
  });
});
