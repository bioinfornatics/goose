import { describe, expect, it } from 'vitest';
import { humanizeToolName } from './PermissionRulesModal';

describe('permission tool labels', () => {
  it('hides the extension prefix from the primary label', () => {
    expect(humanizeToolName('developer__shell')).toBe('Shell');
    expect(humanizeToolName('github__create_pull_request')).toBe('Create Pull Request');
  });

  it('handles unprefixed and dashed MCP tool names', () => {
    expect(humanizeToolName('read-resource')).toBe('Read Resource');
    expect(humanizeToolName('search')).toBe('Search');
  });
});
