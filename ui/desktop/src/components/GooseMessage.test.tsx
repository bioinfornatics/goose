import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { Message } from '../types/message';
import { formatMessageTimestamp } from '../utils/timeUtils';
import { IntlTestWrapper } from '../i18n/test-utils';
import GooseMessage from './GooseMessage';

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

function renderMessage(message: Message, isTurnFinal: boolean) {
  return render(
    <GooseMessage
      sessionId="test-session"
      message={message}
      hideTimestamp={false}
      toolStates={[
        { requestId: 'call-1', response: undefined, confirmation: undefined, isPending: false },
      ]}
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
  it('keeps the timestamp on a turn that ends on a tool call', () => {
    renderMessage(toolOnlyMessage(), true);

    expect(screen.getByText(formatMessageTimestamp(created))).toBeTruthy();
  });

  it('renders nothing for an intermediate message whose calls are all collapsed', () => {
    const { container } = renderMessage(toolOnlyMessage(), false);

    expect(container.innerHTML).toBe('');
  });
});
