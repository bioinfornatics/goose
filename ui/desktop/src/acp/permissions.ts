import type {
  ToolListItem,
  ToolPermissionEntry,
  ToolPermissionLevel,
  ToolPermissionScope,
} from '@aaif/goose-acp-client';
import { getAcpClient } from './acpConnection';

export type { ToolListItem, ToolPermissionEntry, ToolPermissionLevel, ToolPermissionScope };

export interface ToolListResult {
  tools: ToolListItem[];
  writablePermissionScopes: ToolPermissionScope[];
}

export async function listToolsWithScopes(
  sessionId: string,
  extensionName?: string
): Promise<ToolListResult> {
  const client = await getAcpClient();
  const response = await client.goose.toolsList_unstable({
    sessionId,
    extensionName: extensionName ?? null,
  });
  return {
    tools: response.tools ?? [],
    writablePermissionScopes: response.writablePermissionScopes ?? ['user'],
  };
}

export async function listTools(
  sessionId: string,
  extensionName?: string
): Promise<ToolListItem[]> {
  return (await listToolsWithScopes(sessionId, extensionName)).tools;
}

export async function setToolPermissions(
  toolPermissions: ToolPermissionEntry[],
  sessionId?: string
): Promise<void> {
  const client = await getAcpClient();
  await client.goose.toolsPermissionsSet_unstable({
    toolPermissions,
    sessionId: sessionId || null,
  });
}
