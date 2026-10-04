import { describe, expect, it } from 'vitest';
import {
  humanizeToolName,
  sessionExtensionNames,
  toolMetadataLabels,
} from './PermissionRulesModal';
import type { SessionExtension } from '../../../acp/session-extensions';
import type { ToolListItem } from '../../../acp/permissions';

describe('permission tool labels', () => {
  it('hides the extension prefix from the primary label', () => {
    expect(humanizeToolName('developer__shell')).toBe('Shell');
    expect(humanizeToolName('github__create_pull_request')).toBe('Create Pull Request');
  });

  it('handles unprefixed and dashed MCP tool names', () => {
    expect(humanizeToolName('read-resource')).toBe('Read Resource');
    expect(humanizeToolName('search')).toBe('Search');
  });

  it('labels extension-declared hints without presenting them as guarantees', () => {
    const tool = {
      metadataHints: { readOnly: true, destructive: false, openWorld: true },
    } as ToolListItem;

    expect(toolMetadataLabels(tool)).toEqual([
      'Read only · Declared by extension',
      'External interaction · Declared by extension',
    ]);
  });

  it('recommends approval when impact metadata is missing', () => {
    const tool = { metadataHints: {} } as ToolListItem;
    expect(toolMetadataLabels(tool)).toEqual(['Impact not declared · Approval recommended']);
  });

  it('uses only the extensions returned for the active session', () => {
    const extensions = [
      { name: 'github', extensionKey: 'github' },
      { name: 'developer', extensionKey: 'developer' },
    ] as SessionExtension[];

    expect(sessionExtensionNames(extensions)).toEqual(['developer', 'github']);
    expect(sessionExtensionNames(extensions)).not.toContain('configured-but-disabled');
  });
});
