/**
 * The sidebar's grouping of a source's tools: the groups in display order,
 * each with the tools the source offers in it, empty groups dropped. The
 * backend decides which tools a source has; this only arranges them.
 */

import { TOOL_GROUPS, type ToolDef, type ToolGroup } from '@/types';

export interface ToolGroupEntry {
  id: ToolGroup;
  label: string;
  /** Whether the sidebar shows the group's label above its tools. */
  heading: boolean;
  tools: ToolDef[];
}

export function groupTools(tools: ToolDef[]): ToolGroupEntry[] {
  return TOOL_GROUPS.map((group) => ({
    heading: group.heading,
    id: group.id,
    label: group.label,
    tools: tools.filter((tool) => tool.group === group.id),
  })).filter((group) => group.tools.length > 0);
}
