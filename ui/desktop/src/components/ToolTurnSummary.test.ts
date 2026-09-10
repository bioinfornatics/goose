import { describe, expect, it } from 'vitest';
import type { Message, MessageContent } from '../types/message';
import { buildTurnItems } from './ToolTurnSummary';

function assistantMessage(id: string, content: MessageContent[]): Message {
  return {
    id,
    role: 'assistant',
    created: 1,
    content,
    metadata: { agentVisible: true, userVisible: true },
  };
}

function thinking(text: string): MessageContent {
  return { type: 'thinking', thinking: text, signature: '' };
}

function toolRequest(id: string, name: string): MessageContent {
  return {
    type: 'toolRequest',
    id,
    toolCall: { status: 'success', value: { name, arguments: {} } },
  };
}

describe('buildTurnItems', () => {
  it('keeps thinking and tool calls in the order they appear in a message', () => {
    const message = assistantMessage('m1', [
      thinking('first thought'),
      toolRequest('t1', 'shell'),
      thinking('second thought'),
    ]);

    const items = buildTurnItems([message], [message]);

    expect(items.map((item) => item.kind)).toEqual(['thinking', 'tool', 'thinking']);
  });

  it('does not merge adjacent thinking blocks into one item', () => {
    const message = assistantMessage('m1', [thinking('one'), thinking('two')]);

    const items = buildTurnItems([message], [message]);

    expect(items.map((item) => (item.kind === 'thinking' ? item.content : null))).toEqual([
      'one',
      'two',
    ]);
  });

  it('skips a call awaiting approval, which GooseMessage renders in place', () => {
    const message = assistantMessage('m1', [
      toolRequest('t1', 'shell'),
      toolRequest('t2', 'shell'),
    ]);
    const confirmation = assistantMessage('m2', [
      {
        type: 'toolConfirmationRequest',
        id: 't2',
        toolName: 'shell',
        arguments: {},
        prompt: '',
      },
    ]);

    const items = buildTurnItems([message], [message, confirmation]);

    expect(items.map((item) => item.key)).toEqual(['t1']);
  });
});
