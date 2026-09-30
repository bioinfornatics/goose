import { useCallback, useState } from 'react';
import { getToolRequests, getToolResponses, type Message } from '../types/message';
import { nameOf, titleOf, turnStartIndex } from '../components/toolTurnUtils';
import { getToolName } from '../utils/toolPresentation';

export function useToolTurnCollapse(messages: Message[]) {
  const [overrides, setOverrides] = useState<Map<number, boolean>>(new Map());

  const toggleTurn = useCallback((startIndex: number, defaultExpanded = false) => {
    setOverrides((previous) => {
      const next = new Map(previous);
      next.set(startIndex, !(next.get(startIndex) ?? defaultExpanded));
      return next;
    });
  }, []);

  const isTurnExpanded = useCallback(
    (index: number, active = false) => overrides.get(turnStartIndex(messages, index)) ?? active,
    [messages, overrides]
  );

  return { isTurnExpanded, toggleTurn };
}

export function useRunningToolLabel(messages: Message[]): string | undefined {
  const turnStart = turnStartIndex(messages, messages.length - 1);
  const answered = new Set<string>();
  for (let i = turnStart; i < messages.length; i++) {
    for (const response of getToolResponses(messages[i])) answered.add(response.id);
  }
  for (let i = messages.length - 1; i >= turnStart; i--) {
    const requests = getToolRequests(messages[i]);
    for (let j = requests.length - 1; j >= 0; j--) {
      if (answered.has(requests[j].id)) continue;
      const name = nameOf(requests[j]);
      if (name) return titleOf(requests[j]) ?? getToolName(name);
    }
  }
  return undefined;
}
