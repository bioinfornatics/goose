import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Message, MessageContent } from '../types/message';
import { IntlTestWrapper } from '../i18n/test-utils';
import ToolTurnSummary from './ToolTurnSummary';

vi.mock('./MarkdownContent', () => ({
  default: ({ content }: { content: string }) => <div>{content}</div>,
}));

vi.mock('./ToolCallWithResponse', () => ({
  default: () => <div>tool details</div>,
}));

const metadata: Message['metadata'] = { agentVisible: true, userVisible: true };

function message(content: MessageContent[]): Message {
  return {
    id: 'assistant-thinking',
    role: 'assistant',
    created: Math.floor(Date.now() / 1000),
    content,
    metadata,
  };
}

function toolRequest(): MessageContent {
  return {
    type: 'toolRequest',
    id: 'tool-1',
    toolCall: { status: 'success', value: { name: 'developer__shell', arguments: {} } },
  };
}

interface HarnessProps {
  content: MessageContent[];
  initialExpanded?: boolean;
  isStreaming: boolean;
  messagesOverride?: Message[];
}

function Harness({
  content,
  initialExpanded = false,
  isStreaming,
  messagesOverride,
}: HarnessProps) {
  const [isExpanded, setIsExpanded] = useState(initialExpanded);
  const messages = messagesOverride ?? [message(content)];

  return (
    <ToolTurnSummary
      messages={messages}
      responsesById={new Map()}
      pendingApprovalIds={new Set()}
      turnMessages={messages}
      sessionId="session-1"
      toolCallNotifications={new Map()}
      append={vi.fn()}
      isExpanded={isExpanded}
      isStreaming={isStreaming}
      onToggle={() => setIsExpanded((expanded) => !expanded)}
    />
  );
}

describe('ToolTurnSummary thinking preview', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows a two-line active thinking preview while the timeline is collapsed', () => {
    const thinking = 'A long current reasoning message that remains available in full when opened.';
    render(<Harness content={[{ type: 'thinking', thinking, signature: '' }]} isStreaming />, {
      wrapper: IntlTestWrapper,
    });

    const preview = screen.getByRole('button', { name: `Thinking: ${thinking}` });
    expect(preview).toHaveAttribute('aria-expanded', 'false');
    expect(preview).toHaveAttribute('aria-controls');
    expect(preview.querySelector('.line-clamp-2')).not.toBeNull();
  });

  it('keeps the active thinking preview visible while the timeline is expanded', () => {
    const thinking = 'Current reasoning remains visible';
    render(
      <Harness
        content={[{ type: 'thinking', thinking, signature: '' }]}
        initialExpanded
        isStreaming
      />,
      { wrapper: IntlTestWrapper }
    );

    const preview = screen.getByRole('button', { name: `Thinking: ${thinking}` });
    expect(preview).toBeTruthy();
    expect(preview).toHaveAttribute('aria-expanded', 'false');
    expect(preview).toHaveAttribute('aria-controls');
  });

  it('opens the timeline and latest thinking block when the preview is clicked', () => {
    const previous = 'Previous reasoning message';
    const latest = 'Latest reasoning message shown in the preview';
    render(
      <Harness
        content={[
          { type: 'thinking', thinking: previous, signature: '' },
          { type: 'thinking', thinking: latest, signature: '' },
        ]}
        isStreaming
      />,
      { wrapper: IntlTestWrapper }
    );

    fireEvent.click(screen.getByRole('button', { name: `Thinking: ${latest}` }));

    const latestDetailsId = screen
      .getByRole('button', { name: `Thinking: ${latest}` })
      .getAttribute('aria-controls');
    expect(latestDetailsId).not.toBeNull();
    expect(document.getElementById(latestDetailsId!)).toHaveAttribute('open');
    const previousDetails = screen
      .getAllByText(previous)
      .map((node) => node.closest('details'))
      .find(Boolean);
    expect(previousDetails).not.toHaveAttribute('open');
  });

  it('opens only the latest thinking block when the timeline is already expanded', () => {
    const previous = 'Earlier expanded-timeline reasoning';
    const latest = 'Current expanded-timeline reasoning';
    render(
      <Harness
        content={[
          { type: 'thinking', thinking: previous, signature: '' },
          { type: 'thinking', thinking: latest, signature: '' },
        ]}
        initialExpanded
        isStreaming
      />,
      { wrapper: IntlTestWrapper }
    );

    const preview = screen.getByRole('button', { name: `Thinking: ${latest}` });
    fireEvent.click(preview);

    const latestDetailsId = screen
      .getByRole('button', { name: `Thinking: ${latest}` })
      .getAttribute('aria-controls');
    expect(latestDetailsId).not.toBeNull();
    expect(document.getElementById(latestDetailsId!)).toHaveAttribute('open');
    const previousDetails = screen
      .getAllByText(previous)
      .map((node) => node.closest('details'))
      .find(Boolean);
    expect(previousDetails).not.toHaveAttribute('open');
    expect(screen.getByRole('button', { name: `Thinking: ${latest}` })).toHaveAttribute(
      'aria-expanded',
      'true'
    );
    expect(screen.getAllByRole('button')[0]).toHaveAttribute('aria-expanded', 'true');
  });

  it('removes the preview when thinking ends', () => {
    const thinking = 'Transient reasoning message';
    const content: MessageContent[] = [{ type: 'thinking', thinking, signature: '' }];
    const { rerender } = render(<Harness content={content} isStreaming />, {
      wrapper: IntlTestWrapper,
    });
    expect(screen.getByRole('button', { name: `Thinking: ${thinking}` })).toBeTruthy();

    rerender(<Harness content={content} isStreaming={false} />);

    expect(screen.queryByRole('button', { name: `Thinking: ${thinking}` })).toBeNull();
    expect(screen.queryByText(thinking)).toBeNull();
  });

  it('keeps the preview while a tool runs after thinking', () => {
    const thinking = 'Reasoning before a command';
    render(
      <Harness
        content={[{ type: 'thinking', thinking, signature: '' }, toolRequest()]}
        isStreaming
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(screen.getByRole('button', { name: `Thinking: ${thinking}` })).toBeTruthy();
  });

  it('replaces the preview when a newer thinking message arrives after a tool', () => {
    const previous = 'Reasoning before the tool';
    const latest = 'New reasoning after the tool';
    render(
      <Harness
        content={[
          { type: 'thinking', thinking: previous, signature: '' },
          toolRequest(),
          { type: 'thinking', thinking: latest, signature: '' },
        ]}
        isStreaming
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(screen.getByRole('button', { name: `Thinking: ${latest}` })).toBeTruthy();
    expect(screen.queryByRole('button', { name: `Thinking: ${previous}` })).toBeNull();
  });

  it('keeps the preview while an empty response text block is waiting for content', () => {
    const thinking = 'Reasoning before the response';
    render(
      <Harness
        content={[
          { type: 'thinking', thinking, signature: '' },
          { type: 'text', text: '' },
        ]}
        isStreaming
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(screen.getByRole('button', { name: `Thinking: ${thinking}` })).toBeTruthy();
  });

  it('removes the preview when response content starts', () => {
    const thinking = 'Reasoning before response content';
    render(
      <Harness
        content={[
          { type: 'thinking', thinking, signature: '' },
          { type: 'text', text: 'The response has started.' },
        ]}
        isStreaming
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(screen.queryByRole('button', { name: `Thinking: ${thinking}` })).toBeNull();
  });

  it('opens the same updated thinking block that is visible in the preview', () => {
    const first = 'First thinking block';
    const second = 'Second thinking block';
    const { rerender } = render(
      <Harness content={[{ type: 'thinking', thinking: first, signature: '' }]} isStreaming />,
      { wrapper: IntlTestWrapper }
    );

    rerender(
      <Harness content={[{ type: 'thinking', thinking: second, signature: '' }]} isStreaming />
    );
    expect(screen.queryByRole('button', { name: `Thinking: ${first}` })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: `Thinking: ${second}` }));
    const detailsId = screen
      .getByRole('button', { name: `Thinking: ${second}` })
      .getAttribute('aria-controls');
    expect(detailsId).not.toBeNull();
    expect(document.getElementById(detailsId!)).toHaveAttribute('open');
  });

  it('does not preview thinking from a hidden message', () => {
    const hiddenThinking = message([
      { type: 'thinking', thinking: 'Hidden reasoning message', signature: '' },
    ]);
    hiddenThinking.metadata = { ...metadata, userVisible: false };
    const visibleThinking = message([
      { type: 'thinking', thinking: 'Visible reasoning message', signature: '' },
    ]);

    render(
      <Harness content={[]} messagesOverride={[visibleThinking, hiddenThinking]} isStreaming />,
      { wrapper: IntlTestWrapper }
    );

    expect(
      screen.getByRole('button', { name: 'Thinking: Visible reasoning message' })
    ).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Thinking: Hidden reasoning message' })).toBeNull();
  });
});
