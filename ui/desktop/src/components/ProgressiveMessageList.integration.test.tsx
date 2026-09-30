import { useEffect, useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import type { Message, MessageContent } from '../types/message';
import { IntlTestWrapper } from '../i18n/test-utils';
import ProgressiveMessageList from './ProgressiveMessageList';

const appUnmounted = vi.hoisted(() => vi.fn());
vi.mock('./McpApps/McpAppRenderer', () => ({
  default: function StatefulApp() {
    const [value, setValue] = useState('');
    useEffect(() => () => appUnmounted(), []);
    return (
      <input
        aria-label="App draft"
        value={value}
        onChange={(event) => setValue(event.target.value)}
      />
    );
  },
}));

const created = 1758000000;
const metadata = { agentVisible: true, userVisible: true };
function message(id: string, role: Message['role'], content: MessageContent[]): Message {
  return { id, role, created, content, metadata };
}
const user = message('user', 'user', [{ type: 'text', text: 'Investigate' }]);
const thought: MessageContent = {
  type: 'thinking',
  thinking: 'Checking the evidence',
  signature: '',
};
const answer = message('answer', 'assistant', [
  thought,
  { type: 'text', text: 'The answer is 42.' },
]);
const notice = message('notice', 'assistant', [
  { type: 'systemNotification', notificationType: 'inlineMessage', msg: 'Stop hook limit reached' },
]);
const isUserMessage = (item: Message) => item.role === 'user';
function list(messages: Message[], streaming = false) {
  return (
    <ProgressiveMessageList
      messages={messages}
      sessionId="test-session"
      isUserMessage={isUserMessage}
      isStreamingMessage={streaming}
    />
  );
}

beforeEach(() => appUnmounted.mockClear());

afterEach(() => {
  vi.useRealTimers();
  appUnmounted.mockClear();
});

describe('turn summaries with real message rendering', () => {
  it('keeps the final answer exactly once after a terminal notification', () => {
    render(list([user, answer, notice]), { wrapper: IntlTestWrapper });
    expect(screen.getAllByText('The answer is 42.')).toHaveLength(1);
    expect(screen.getByText('Stop hook limit reached')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Completed' }));
    expect(screen.getAllByText('The answer is 42.')).toHaveLength(1);
  });

  it('does not duplicate multiple text blocks from the final message in the timeline', () => {
    const multiple = {
      ...answer,
      content: [
        thought,
        { type: 'text' as const, text: 'First part. ' },
        { type: 'text' as const, text: 'Second part.' },
      ],
    };
    render(list([user, multiple, notice]), { wrapper: IntlTestWrapper });
    fireEvent.click(screen.getByRole('button', { name: 'Completed' }));
    expect(screen.getAllByText('First part. Second part.')).toHaveLength(1);
    expect(screen.queryByText('First part.')).toBeNull();
  });

  it('keeps the text when an image-only message follows it', () => {
    const image = message('image', 'assistant', [
      { type: 'image', data: 'aW1hZ2U=', mimeType: 'image/png' },
    ]);
    render(list([user, answer, image]), { wrapper: IntlTestWrapper });
    expect(screen.getByText('The answer is 42.')).toBeVisible();
  });

  it('ignores hidden messages when selecting and building the summary', () => {
    const hidden = {
      ...message('hidden', 'assistant', [{ type: 'text', text: 'Hidden text' }, thought]),
      metadata: { ...metadata, userVisible: false },
    };
    render(list([user, answer, hidden]), { wrapper: IntlTestWrapper });
    fireEvent.click(screen.getByRole('button', { name: 'Completed' }));
    expect(screen.getAllByText('The answer is 42.')).toHaveLength(1);
    expect(screen.queryByText('Hidden text')).toBeNull();
    expect(screen.getAllByText('Checking the evidence')).toHaveLength(1);
  });

  it.each([false, true])(
    'preserves an MCP app at completion (manual toggle: %s)',
    (manualToggle) => {
      vi.useFakeTimers();
      vi.setSystemTime(created * 1000);
      const request = message('request', 'assistant', [
        {
          type: 'toolRequest',
          id: 'app-call',
          toolCall: { status: 'success', value: { name: 'demo__show', arguments: {} } },
        },
      ]);
      const response = message('response', 'user', [
        {
          type: 'toolResponse',
          id: 'app-call',
          toolResult: {
            status: 'success',
            value: {
              content: [],
              isError: false,
              _meta: {
                ui: { resourceUri: 'ui://demo/app' },
                extensionName: 'demo',
                toolName: 'show',
              },
            },
          },
        },
      ]);
      const messages = [user, request, response, answer];
      const { rerender } = render(list(messages, true), { wrapper: IntlTestWrapper });
      const input = screen.getByRole('textbox', { name: 'App draft' });
      fireEvent.change(input, { target: { value: 'Unsaved work' } });
      const summary = screen.getByRole('button', { name: 'Thinking' });
      if (manualToggle) {
        fireEvent.click(summary);
        expect(input).toBeVisible();
        fireEvent.click(summary);
      }
      act(() => {
        vi.advanceTimersByTime(10000);
      });
      rerender(list(messages, false));
      expect(screen.getByRole('textbox', { name: 'App draft' })).toBe(input);
      expect(input).toHaveValue('Unsaved work');
      expect(input).toBeVisible();
      expect(summary).toHaveAttribute('aria-expanded', String(manualToggle));
      expect(appUnmounted).not.toHaveBeenCalled();
      expect(screen.getAllByRole('textbox', { name: 'App draft' })).toHaveLength(1);
      expect(screen.getByRole('button', { name: 'Completed in 10 sec' })).toHaveAttribute(
        'aria-expanded',
        String(manualToggle)
      );
    }
  );

  it('measures a single streamed message and freezes its duration on completion', () => {
    vi.useFakeTimers();
    vi.setSystemTime(created * 1000);
    const { rerender } = render(list([user, answer], true), { wrapper: IntlTestWrapper });
    act(() => {
      vi.advanceTimersByTime(12000);
    });
    expect(screen.getByText('12 sec elapsed')).toBeVisible();
    rerender(list([user, answer], false));
    expect(screen.getByRole('button', { name: 'Completed in 12 sec' })).toBeVisible();
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByRole('button', { name: 'Completed in 12 sec' })).toBeVisible();
  });

  it('keeps the answer when a tool response arrives after it', () => {
    const response = message('late-response', 'user', [
      {
        type: 'toolResponse',
        id: 'late-call',
        toolResult: { status: 'success', value: { content: [], isError: false } },
      },
    ]);
    render(list([user, answer, response, notice]), { wrapper: IntlTestWrapper });
    expect(screen.getByText('The answer is 42.')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Completed' }));
    expect(screen.getAllByText('The answer is 42.')).toHaveLength(1);
  });

  it('shows a failed historical turn without inventing a duration', () => {
    const request = message('request', 'assistant', [
      {
        type: 'toolRequest',
        id: 'failed-call',
        toolCall: { status: 'success', value: { name: 'shell', arguments: {} } },
      },
    ]);
    const response = message('response', 'user', [
      {
        type: 'toolResponse',
        id: 'failed-call',
        toolResult: { status: 'error', error: 'Command failed' },
      },
    ]);
    render(list([user, request, response]), { wrapper: IntlTestWrapper });
    expect(screen.getByRole('button', { name: 'Failed' })).toBeVisible();
    expect(screen.queryByText(/Failed after/)).toBeNull();
  });

  it('does not invent a duration for a reloaded historical turn', () => {
    render(list([user, answer]), { wrapper: IntlTestWrapper });
    expect(screen.getByRole('button', { name: 'Completed' })).toBeVisible();
    expect(screen.queryByText(/Completed in/)).toBeNull();
  });
});

describe('inactive tool turn status', () => {
  const request = message('tool-request', 'assistant', [
    {
      type: 'toolRequest',
      id: 'unfinished',
      toolCall: { status: 'success', value: { name: 'demo__shell', arguments: {} } },
    },
  ]);

  it('shows incomplete for a historical unanswered tool rather than completed', () => {
    render(list([user, request]), { wrapper: IntlTestWrapper });
    expect(screen.getByRole('button', { name: 'Incomplete' })).toBeVisible();
    expect(screen.queryByRole('button', { name: /Completed/ })).toBeNull();
  });

  it('shows incomplete when a live unanswered tool becomes inactive', () => {
    vi.useFakeTimers();
    vi.setSystemTime(created * 1000);
    const { rerender } = render(list([user, request], true), { wrapper: IntlTestWrapper });
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    rerender(list([user, request], false));
    expect(screen.getByRole('button', { name: 'Incomplete' })).toBeVisible();
    expect(screen.queryByRole('button', { name: /Completed/ })).toBeNull();
  });

  it('preserves failed status even if another call has no response', () => {
    const failedRequest = message('failed-request', 'assistant', [
      {
        type: 'toolRequest',
        id: 'failed',
        toolCall: { status: 'success', value: { name: 'demo__shell', arguments: {} } },
      },
    ]);
    const failedResponse = message('failed-response', 'user', [
      {
        type: 'toolResponse',
        id: 'failed',
        toolResult: { status: 'error', error: 'Failure' },
      },
    ]);
    render(list([user, failedRequest, failedResponse, request]), { wrapper: IntlTestWrapper });
    expect(screen.getByRole('button', { name: 'Failed' })).toBeVisible();
  });

  it('preserves approval status when another tool is incomplete', () => {
    const approvalRequest = message('approval-request', 'assistant', [
      {
        type: 'toolRequest',
        id: 'approval-call',
        toolCall: { status: 'success', value: { name: 'demo__shell', arguments: {} } },
      },
    ]);
    const approval = message('approval', 'assistant', [
      {
        type: 'actionRequired',
        data: {
          actionType: 'toolConfirmation',
          id: 'approval-call',
          toolName: 'demo__shell',
          arguments: {},
        },
      },
    ]);
    render(list([user, request, approvalRequest, approval]), { wrapper: IntlTestWrapper });
    expect(screen.getByRole('button', { name: 'Waiting for approval' })).toBeVisible();
  });
});
