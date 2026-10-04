import { describe, expect, it } from 'vitest';
import {
  humanizeToolName,
  permissionAtScope,
  resolvePermissionSessionId,
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

  it('surfaces deterministic shell policy rules without hiding their limitation', () => {
    const tool = {
      shellPolicy: {
        enforcement: 'Built-in deterministic policy',
        rules: [{ decision: 'deny', pattern: 'sudo | doas | su', reason: 'Forbidden' }],
        limitations: 'Not an OS sandbox',
      },
    } as ToolListItem;

    expect(tool.shellPolicy?.rules[0]).toMatchObject({
      decision: 'deny',
      pattern: 'sudo | doas | su',
    });
    expect(tool.shellPolicy?.limitations).toContain('sandbox');
  });

  it('prefers the authoritative ChatInput session ID over stale context', () => {
    expect(resolvePermissionSessionId('new-session', '')).toBe('new-session');
    expect(resolvePermissionSessionId(undefined, 'context-session')).toBe('context-session');
  });

  it('selects the explicit rule for the chosen persistence scope', () => {
    const tool = {
      name: 'developer__shell',
      extensionName: 'developer',
      applicablePermissionRules: [
        {
          scope: 'user',
          effect: 'always_allow',
          principal: { type: 'function', extension: 'developer', function: 'shell' },
          origin: 'user',
        },
        {
          scope: 'session',
          effect: 'ask_before',
          principal: { type: 'function', extension: 'developer', function: 'shell' },
          origin: 'session',
        },
      ],
    } as ToolListItem;

    expect(permissionAtScope(tool, 'session')).toBe('ask_before');
    expect(permissionAtScope(tool, 'project_local')).toBe('default');
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
