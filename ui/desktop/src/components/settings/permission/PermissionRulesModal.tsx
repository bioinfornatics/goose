import { useCallback, useEffect, useMemo, useState } from 'react';
import { AlertCircle, ChevronDown, ChevronRight, Search, SlidersHorizontal } from 'lucide-react';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '../../ui/dialog';
import { Input } from '../../ui/input';
import { Button } from '../../ui/button';
import { FixedExtensionEntry, useConfig } from '../../ConfigContext';
import { useChatContext } from '../../../contexts/ChatContext';
import { listTools, setToolPermissions } from '../../../acp/permissions';
import type { ToolListItem, ToolPermissionLevel } from '../../../acp/permissions';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: { id: 'permissionRulesModal.title', defaultMessage: 'Permission Rules' },
  description: {
    id: 'permissionRulesModal.description',
    defaultMessage: 'Control each function exposed by your active extensions and MCP servers.',
  },
  search: {
    id: 'permissionRulesModal.search',
    defaultMessage: 'Search extensions, tools, or actions…',
  },
  defaultPermission: { id: 'permissionRulesModal.default', defaultMessage: 'Use mode default' },
  alwaysAllow: { id: 'permissionRulesModal.alwaysAllow', defaultMessage: 'Always allow' },
  askBefore: { id: 'permissionRulesModal.askBefore', defaultMessage: 'Ask before' },
  neverAllow: { id: 'permissionRulesModal.neverAllow', defaultMessage: 'Never allow' },
  save: { id: 'permissionRulesModal.save', defaultMessage: 'Save changes' },
  cancel: { id: 'permissionRulesModal.cancel', defaultMessage: 'Cancel' },
  noSession: {
    id: 'permissionRulesModal.noSession',
    defaultMessage: 'Start a session to manage its tools.',
  },
  noResults: { id: 'permissionRulesModal.noResults', defaultMessage: 'No matching tools.' },
  loadFailed: {
    id: 'permissionRulesModal.loadFailed',
    defaultMessage: 'This extension could not be loaded.',
  },
});

type PermissionChoice = ToolPermissionLevel | 'default';
type ExtensionTools = { extension: FixedExtensionEntry; tools: ToolListItem[]; failed: boolean };

export function humanizeToolName(name: string): string {
  const rawName = name.split('__').at(-1) ?? name;
  return rawName.replace(/[_-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function firstSentence(value: string): string {
  return value.match(/^([^.?!]+[.?!])/)?.[0] ?? value;
}

export function toolMetadataLabels(tool: ToolListItem): string[] {
  const labels: string[] = [];
  if (tool.metadataHints.readOnly === true) labels.push('Read only · Declared by extension');
  if (tool.metadataHints.destructive === true) {
    labels.push('Potentially destructive · Declared by extension');
  }
  if (tool.metadataHints.openWorld === true) {
    labels.push('External interaction · Declared by extension');
  }
  if (tool.metadataHints.readOnly == null && tool.metadataHints.destructive == null) {
    labels.push('Impact not declared · Approval recommended');
  }
  return labels;
}

interface PermissionRulesModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PermissionRulesModal({ isOpen, onClose }: PermissionRulesModalProps) {
  const intl = useIntl();
  const { getExtensions } = useConfig();
  const sessionId = useChatContext()?.chat.sessionId ?? '';
  const [groups, setGroups] = useState<ExtensionTools[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [changes, setChanges] = useState<Record<string, PermissionChoice>>({});
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    if (!isOpen || !sessionId) return;
    setLoading(true);
    const entries = (await getExtensions(true)).filter((extension) => extension.enabled);
    if (!entries.some((extension) => extension.name === 'platform')) {
      entries.push({
        name: 'platform',
        type: 'builtin',
        description: 'Built-in tools',
        enabled: true,
      });
    }
    const loaded = await Promise.all(
      entries.map(async (extension): Promise<ExtensionTools> => {
        try {
          const tools = (await listTools(sessionId, extension.name)).filter(
            (tool) => !['platform__read_resource', 'platform__list_resources'].includes(tool.name)
          );
          return { extension, tools, failed: false };
        } catch {
          return { extension, tools: [], failed: true };
        }
      })
    );
    const visible = loaded
      .filter((group) => group.failed || group.tools.length > 0)
      .sort((a, b) => a.extension.name.localeCompare(b.extension.name));
    setGroups(visible);
    setExpanded(new Set(visible.map((group) => group.extension.name)));
    setLoading(false);
  }, [getExtensions, isOpen, sessionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return groups;
    return groups
      .map((group) => ({
        ...group,
        tools: group.tools.filter((tool) =>
          [group.extension.name, tool.name, humanizeToolName(tool.name), tool.description]
            .join(' ')
            .toLowerCase()
            .includes(needle)
        ),
      }))
      .filter(
        (group) => group.tools.length > 0 || group.extension.name.toLowerCase().includes(needle)
      );
  }, [groups, query]);

  const save = async () => {
    setSaving(true);
    try {
      await setToolPermissions(
        Object.entries(changes).map(([toolName, permission]) => ({
          toolName,
          permission: permission === 'default' ? null : permission,
        }))
      );
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const options: Array<{ value: PermissionChoice; label: string }> = [
    { value: 'default', label: intl.formatMessage(i18n.defaultPermission) },
    { value: 'always_allow', label: intl.formatMessage(i18n.alwaysAllow) },
    { value: 'ask_before', label: intl.formatMessage(i18n.askBefore) },
    { value: 'never_allow', label: intl.formatMessage(i18n.neverAllow) },
  ];

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-[880px] max-h-[85vh] p-0 flex flex-col overflow-hidden">
        <DialogHeader className="px-8 pt-6 pb-4">
          <DialogTitle className="flex items-center gap-3 text-2xl">
            <SlidersHorizontal aria-hidden="true" />
            {intl.formatMessage(i18n.title)}
          </DialogTitle>
          <p className="text-sm text-text-secondary">{intl.formatMessage(i18n.description)}</p>
          <div className="relative pt-2">
            <Search
              className="absolute left-3 top-4 h-4 w-4 text-text-secondary"
              aria-hidden="true"
            />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={intl.formatMessage(i18n.search)}
              aria-label={intl.formatMessage(i18n.search)}
              className="pl-9"
            />
          </div>
        </DialogHeader>
        <div className="flex-1 overflow-y-auto px-8 pb-6">
          {!sessionId ? (
            <p className="py-8 text-center text-text-secondary">
              {intl.formatMessage(i18n.noSession)}
            </p>
          ) : loading ? (
            <p className="py-8 text-center text-text-secondary" role="status">
              Loading tools…
            </p>
          ) : filtered.length === 0 ? (
            <p className="py-8 text-center text-text-secondary">
              {intl.formatMessage(i18n.noResults)}
            </p>
          ) : (
            <div className="space-y-3">
              {filtered.map(({ extension, tools, failed }) => {
                const isExpanded = expanded.has(extension.name) || Boolean(query);
                return (
                  <section key={extension.name} className="rounded-lg border border-border-primary">
                    <button
                      type="button"
                      className="flex w-full items-center justify-between p-4 text-left"
                      aria-expanded={isExpanded}
                      onClick={() =>
                        setExpanded((current) => {
                          const next = new Set(current);
                          next.has(extension.name)
                            ? next.delete(extension.name)
                            : next.add(extension.name);
                          return next;
                        })
                      }
                    >
                      <span>
                        <span className="block font-medium text-text-primary">
                          {extension.name}
                        </span>
                        <span className="text-xs text-text-secondary">
                          {failed
                            ? intl.formatMessage(i18n.loadFailed)
                            : tools.length + ' function' + (tools.length === 1 ? '' : 's')}
                        </span>
                      </span>
                      {isExpanded ? (
                        <ChevronDown aria-hidden="true" />
                      ) : (
                        <ChevronRight aria-hidden="true" />
                      )}
                    </button>
                    {isExpanded && (
                      <div className="border-t border-border-primary px-4">
                        {failed ? (
                          <div className="flex items-center gap-2 py-4 text-sm text-text-secondary">
                            <AlertCircle aria-hidden="true" />
                            {intl.formatMessage(i18n.loadFailed)}
                          </div>
                        ) : (
                          tools.map((tool) => {
                            const value =
                              changes[tool.name] ?? tool.explicitPermission ?? 'default';
                            const effectiveLabel = tool.effectivePermission
                              ? options.find((option) => option.value === tool.effectivePermission)
                                  ?.label
                              : 'Evaluated for each call';
                            const id = 'permission-' + tool.name;
                            return (
                              <div
                                key={tool.name}
                                className="grid grid-cols-[1fr_180px] gap-4 border-b border-border-primary py-4 last:border-b-0"
                              >
                                <div className="min-w-0">
                                  <label htmlFor={id} className="font-medium text-text-primary">
                                    {tool.displayName || humanizeToolName(tool.name)}
                                  </label>
                                  <p className="text-xs font-medium text-text-secondary">
                                    {extension.name} · This function only
                                  </p>
                                  <p className="text-sm text-text-secondary">
                                    {firstSentence(tool.description) ||
                                      'No description was provided by this extension.'}
                                  </p>
                                  <div className="flex flex-wrap gap-1 py-1 text-xs text-text-secondary">
                                    {toolMetadataLabels(tool).map((label) => (
                                      <span key={label}>{label}</span>
                                    ))}
                                  </div>
                                  <p className="text-xs text-text-secondary">
                                    Effective: {effectiveLabel} · {tool.permissionReason}
                                  </p>
                                  <details className="text-xs text-text-secondary">
                                    <summary>Technical details</summary>
                                    <code>{tool.name}</code>
                                    <span className="ml-2">Metadata: {tool.metadataSource}</span>
                                  </details>
                                </div>
                                <select
                                  id={id}
                                  value={value}
                                  onChange={(event) =>
                                    setChanges((current) => ({
                                      ...current,
                                      [tool.name]: event.target.value as PermissionChoice,
                                    }))
                                  }
                                  className="h-9 rounded-md border border-border-primary bg-background-primary px-3 text-sm text-text-primary"
                                >
                                  {options.map((option) => (
                                    <option key={option.value} value={option.value}>
                                      {option.label}
                                    </option>
                                  ))}
                                </select>
                              </div>
                            );
                          })
                        )}
                      </div>
                    )}
                  </section>
                );
              })}
            </div>
          )}
        </div>
        <DialogFooter className="border-t border-border-primary px-8 py-4">
          <Button variant="outline" onClick={onClose}>
            {intl.formatMessage(i18n.cancel)}
          </Button>
          <Button disabled={saving || Object.keys(changes).length === 0} onClick={save}>
            {intl.formatMessage(i18n.save)}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
