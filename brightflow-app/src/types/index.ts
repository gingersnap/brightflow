/**
 * Shared type definitions for Brightflow
 */

// Import generated types used in interfaces below
import type { Aggregation as AggFn, FilterOp } from './generated';

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

// Unified source types (frontend-only, matching backend SourceKind/SourceTool)
export type SourceKind = 'web-analytics' | 'connector';

export type ToolId =
  | 'dashboard'
  | 'funnels'
  | 'retention'
  | 'users'
  | 'explore'
  | 'insights'
  | 'topics'
  | 'settings';

export interface SourceTable {
  name: string;
  numRows: number | null;
  enrichable: boolean;
}

export interface UnifiedSource {
  id: string;
  name: string;
  kind: SourceKind;
  connectorName: string | null;
  domain: string | null;
  tables: SourceTable[];
  tools: ToolId[];
  createdAt: string;
  ready: boolean;
}

export interface ToolDef {
  id: ToolId;
  label: string;
  icon: string;
}

export const TOOL_DEFS: Record<ToolId, { label: string; icon: string }> = {
  dashboard: { label: 'Dashboard', icon: 'i-lucide-bar-chart-3' },
  funnels: { label: 'Funnels', icon: 'i-lucide-git-branch' },
  retention: { label: 'Retention', icon: 'i-lucide-calendar-check' },
  users: { label: 'Users', icon: 'i-lucide-users' },
  explore: { label: 'Explore', icon: 'i-lucide-search' },
  insights: { label: 'Insights', icon: 'i-lucide-sparkles' },
  topics: { label: 'Topics', icon: 'i-lucide-shapes' },
  settings: { label: 'Settings', icon: 'i-lucide-settings' },
};

export function toolsForSource(source: UnifiedSource): ToolDef[] {
  const ids: ToolId[] = [...source.tools.filter((id) => id !== 'settings'), 'settings'];
  return ids.map((id) => {
    const def = TOOL_DEFS[id];
    return { icon: def.icon, id, label: def.label };
  });
}

// Filter state (frontend-only)
export interface Filter {
  id: string;
  column: string | null;
  op: FilterOp | '';
  value: unknown;
}

// Aggregation state (frontend-only)
export interface Aggregation {
  id: string;
  column: string;
  function: AggFn;
  alias: string;
}

// Query section state (frontend-only)
export interface SectionState {
  enabled: boolean;
  collapsed: boolean;
}

export interface QuerySections {
  filter: SectionState;
  select: SectionState;
  groupBy: SectionState;
  pivot: SectionState;
  sort: SectionState;
  limit: SectionState;
}

// Pivot state (frontend-only)
export interface PivotState {
  index: string[];
  columns: string | null;
  values: string | null;
  agg: AggFn;
}

// Pivot field (frontend-only)
export interface PivotField {
  id: string;
  column: string;
  dtype: string;
  aggregation?: AggFn;
}

// Frontend-only pivot operation (extends wire format with UI-only fields)
export interface PivotOperation {
  type: 'pivot';
  index: string[];
  columns: string | null;
  values: string | { column: string; agg: string }[];
  agg: string | string[];
  includeSubtotals?: boolean;
  includeTotals?: boolean;
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

// Aggregation definition (frontend-only)
export interface AggregationDef {
  label: string;
  description: string;
  types: string[];
  usesStar?: boolean;
}

export interface AggregationOption {
  value: string;
  label: string;
  description: string;
  usesStar?: boolean;
}
