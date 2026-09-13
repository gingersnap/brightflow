/**
 * Shared type definitions for Brightflow
 */

// Import generated types used in interfaces below
import type { FieldSort } from '@/utils/pivotOrder';

import type {
  Aggregation as AggFn,
  ColumnRole,
  FilterOp,
  LogicalType,
  SourceTool,
  TimeGranularity,
  UnifiedSource,
} from './generated';

// Re-export generated types from Rust backend
export type { ColumnInfo, Operation, QueryResponse, WsServerMessage } from './generated';
export type { Aggregation as AggFn, FilterOp } from './generated';
// Also re-export remaining generated types used across the app
export type {
  AvailableConnectorResponse,
  BreakdownRow,
  CreateSourceRequest,
  DashboardStats,
  DatasetInfo,
  EnrichedSyncRun,
  EventListRow,
  FunnelRequest,
  FunnelResult,
  FunnelStep,
  FunnelStepResult,
  InsightsResponse,
  LoadTableResponse,
  PresetInfo,
  RetentionRequest,
  RetentionResult,
  RetentionRow,
  RunTriggerResponse,
  ScheduleResponse,
  Source,
  SyncRun,
  TimeseriesPoint,
  UnifiedConnector,
  UnifiedJob,
  UnifiedSyncRun,
  UpdateSourceRequest,
  UploadResponse,
  User,
  UserProfile,
  UserTimelineEvent,
} from './generated';

// Unified source types come from the backend via ts-rs.
export type { SourceKind, SourceTable, SourceTool, UnifiedSource } from './generated';

/**
 * Tool ids the UI routes on: the backend's tools plus the client-side tabs
 * every source gets — Saved (its views), Semantics (what its tables mean)
 * and Settings.
 */
export type ToolId = SourceTool | 'saved' | 'semantics' | 'settings';

/**
 * What a tool is for, which is how the sidebar groups them: the landing
 * view, asking questions of the data, defining what the data means, and
 * administering the source. Analyze and Data carry a heading in the
 * sidebar; Overview and Settings are single entries and need none.
 */
export type ToolGroup = 'overview' | 'analyze' | 'data' | 'settings';

export const TOOL_GROUPS: { id: ToolGroup; label: string; heading: boolean }[] = [
  { heading: false, id: 'overview', label: 'Overview' },
  { heading: true, id: 'analyze', label: 'Analyze' },
  { heading: true, id: 'data', label: 'Data' },
  { heading: false, id: 'settings', label: 'Settings' },
];

export interface ToolDef {
  id: ToolId;
  label: string;
  icon: string;
  group: ToolGroup;
}

export const TOOL_DEFS: Record<ToolId, { label: string; icon: string; group: ToolGroup }> = {
  dashboard: { group: 'overview', icon: 'i-lucide-layout-dashboard', label: 'Overview' },
  funnels: { group: 'analyze', icon: 'i-lucide-git-branch', label: 'Funnels' },
  retention: { group: 'analyze', icon: 'i-lucide-calendar-check', label: 'Retention' },
  users: { group: 'analyze', icon: 'i-lucide-users', label: 'Users' },
  explore: { group: 'analyze', icon: 'i-lucide-search', label: 'Explore' },
  insights: { group: 'analyze', icon: 'i-lucide-sparkles', label: 'Insights' },
  textexplore: { group: 'analyze', icon: 'i-lucide-text-search', label: 'Text Explorer' },
  saved: { group: 'analyze', icon: 'i-lucide-bookmark', label: 'Saved' },
  textenrichment: {
    group: 'data',
    icon: 'i-lucide-messages-square',
    label: 'Text enrichment',
  },
  semantics: { group: 'data', icon: 'i-lucide-book-open-text', label: 'Semantics' },
  settings: { group: 'settings', icon: 'i-lucide-settings', label: 'Settings' },
};

export function toolsForSource(source: UnifiedSource): ToolDef[] {
  // Collision-free by type: the backend's SourceTool can't contain
  // 'saved', 'semantics' or 'settings', which are the client-side tabs.
  const ids: ToolId[] = [...source.tools, 'saved', 'semantics', 'settings'];
  return ids.map((id) => {
    const def = TOOL_DEFS[id];
    return { group: def.group, icon: def.icon, id, label: def.label };
  });
}

// Filter state (frontend-only)
export interface Filter {
  id: string;
  column: string | null;
  op: FilterOp | '';
  value: unknown;
}

// Query section state (frontend-only)
export interface SectionState {
  enabled: boolean;
  collapsed: boolean;
}

export interface QuerySections {
  filter: SectionState;
  sort: SectionState;
  limit: SectionState;
}

// Pivot field (frontend-only)
export interface PivotField {
  id: string;
  column: string;
  datatype: LogicalType;
  /** The column's stored role at the time it was dropped, if any. */
  role?: ColumnRole | null;
  /**
   * Time fields in the row or column bucket: the period they are bucketed
   * into. Set when a time column is dropped; absent on every other field.
   */
  granularity?: TimeGranularity;
  aggregation?: AggFn;
  /** Row and column fields: their display order (see `utils/pivotOrder`). */
  sort?: FieldSort;
}

// Connection status (frontend-only)
export type ConnectionStatus = 'connected' | 'connecting' | 'disconnected' | 'error';

// View modes (frontend-only)
export type ViewMode = 'table' | 'pivot' | 'chart' | 'split' | 'number';
export type ChartType = 'bar' | 'line' | 'pie' | 'scatter';
export type TextSize = 'small' | 'default' | 'large';

// Operator definition (frontend-only)
export interface OperatorDef {
  label: string;
  /** The label for a temporal column, where "after" reads better than "greater than". */
  temporalLabel?: string;
  types: string[];
  noValue?: boolean;
  isArray?: boolean;
}

export interface Operator {
  value: string;
  label: string;
  noValue: boolean;
  isArray: boolean;
}

export interface AggregationOption {
  value: string;
  label: string;
  description: string;
  usesStar?: boolean;
}

// One shown-insight history record (novelty memory). Serialized by the store
// Layer with serde snake_case — not a ts-rs generated type.
export interface InsightHistoryRow {
  table_id: string;
  fingerprint: string;
  identity: string;
  insight_type: string;
  last_value_sig: string;
  shown_count: number;
  first_shown_at: number;
  last_shown_at: number;
}
