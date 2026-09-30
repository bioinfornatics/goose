import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { Message } from '../types/message';
import { formatMessageTimestamp } from '../utils/timeUtils';
import { IntlTestWrapper } from '../i18n/test-utils';
import GooseMessage from './GooseMessage';

vi.mock('./ThinkingContent', () => ({
  default: ({ content }: { content: string }) => <div>thinking:{content}</div>,
}));

vi.mock('./ToolCallWithResponse', () => ({
  default: ({
    toolRequest,
    isPendingApproval,
  }: {
    toolRequest: { id: string };
    isPendingApproval: boolean;
  }) => <div>{`tool:${toolRequest.id}:pending:${isPendingApproval}`}</div>,
}));

const created = 1758000000;

function toolOnlyMessage(usage?: Message['metadata']['usage']): Message {
  return {
    id: 'assistant-1',
    role: 'assistant',
    created,
    content: [
      {
        type: 'toolRequest',
        id: 'call-1',
        toolCall: { status: 'success', value: { name: 'test_tool', arguments: {} } },
      },
    ],
    metadata: { agentVisible: true, userVisible: true, ...(usage ? { usage } : {}) },
  };
}

function renderMessage(
  message: Message,
  isTurnFinal: boolean,
  toolStates = [
    { requestId: 'call-1', response: undefined, confirmation: undefined, isPending: false },
  ]
) {
  return render(
    <GooseMessage
      sessionId="test-session"
      message={message}
      hideTimestamp={false}
      toolStates={toolStates}
      toolNotifications={[undefined]}
      toolConfirmationShownInline={false}
      append={vi.fn()}
      isStreaming={false}
      collapseToolCalls
      isTurnFinal={isTurnFinal}
    />,
    { wrapper: IntlTestWrapper }
  );
}

describe('GooseMessage with collapsed tool calls', () => {
  it('keeps final assistant text while collapsing thinking and completed tools', () => {
    const message: Message = {
      ...toolOnlyMessage(),
      content: [
        { type: 'thinking', thinking: 'private reasoning', signature: 'sig' },
        ...toolOnlyMessage().content,
        { type: 'text', text: 'Final assistant response' },
      ],
    };

    renderMessage(message, true);

    expect(screen.getByText('Final assistant response')).toBeTruthy();
    expect(screen.queryByText('thinking:private reasoning')).toBeNull();
    expect(screen.queryByText('tool:call-1:pending:false')).toBeNull();
  });

  it('keeps a tool awaiting approval inline when tool calls are collapsed', () => {
    renderMessage(toolOnlyMessage(), true, [
      { requestId: 'call-1', response: undefined, confirmation: undefined, isPending: true },
    ]);

    expect(screen.getByText('tool:call-1:pending:true')).toBeTruthy();
  });

  it('keeps the timestamp on a turn that ends on a tool call', () => {
    renderMessage(toolOnlyMessage(), true);

    expect(screen.getByText(formatMessageTimestamp(created))).toBeTruthy();
  });

  it('renders nothing for an intermediate message whose calls are all collapsed', () => {
    const { container } = renderMessage(toolOnlyMessage(), false);

    expect(container.innerHTML).toBe('');
  });
});
