import { describe, expect, it } from 'vitest';
import { getToolDescription, getToolName } from './toolPresentation';

describe('tool presentation', () => {
  it('extracts the tool name and preserves known descriptions', () => {
    expect(getToolName('developer__shell')).toBe('shell');
    expect(getToolDescription('developer__text_editor', { command: 'view', path: '/tmp/a' })).toBe(
      'reading /tmp/a'
    );
  });

  it('truncates commands and generic values', () => {
    const longValue = 'x'.repeat(121);
    expect(getToolDescription('shell', { command: longValue })).toBe(`running ${'x'.repeat(120)}…`);
    expect(getToolDescription('custom_tool', { input: longValue })).toBe(
      `Custom Tool input: ${'x'.repeat(120)}…`
    );
  });

  it('handles malformed arguments safely', () => {
    expect(getToolDescription('custom_tool', null)).toBe('Custom Tool');
    expect(getToolDescription('shell', 'not an object')).toBeNull();

    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(getToolDescription('custom_tool', circular)).toBe('Custom Tool self: [object Object]');
  });

  it('summarizes execute_typescript tool graphs', () => {
    expect(
      getToolDescription('execute_typescript', {
        tool_graph: [{ tool: 'developer/shell', description: 'Inspect files', depends_on: [] }],
      })
    ).toBe('Inspect files');
    expect(
      getToolDescription('execute_typescript', {
        tool_graph: [
          { tool: 'developer/shell', description: 'Inspect', depends_on: [] },
          { tool: 'developer/edit', description: 'Edit', depends_on: [0] },
        ],
      })
    ).toBe('developer/shell, developer/edit');
    expect(getToolDescription('execute_typescript', { tool_graph: [{ description: 42 }] })).toBe(
      'executing code'
    );
  });
});
