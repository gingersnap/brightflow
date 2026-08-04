/**
 * Shared type definitions for Brightflow
 */

// Import generated types used in interfaces below
import type { Aggregation as AggFn, FilterOp, SourceTool, UnifiedSource } from './generated';

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

/** Tool ids the UI routes on: the backend's tools plus the client-side settings tab. */
export type ToolId = SourceTool | 'settings';

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
  textexplore: { label: 'Text Explorer', icon: 'i-lucide-text-search' },
  enrich: { label: 'Enrich', icon: 'i-lucide-wand-sparkles' },
  settings: { label: 'Settings', icon: 'i-lucide-settings' },
};

export function toolsForSource(source: UnifiedSource): ToolDef[] {
  // Collision-free by type: the backend's SourceTool can't contain
  // 'settings', which is the client-side tab.
  const ids: ToolId[] = [...source.tools, 'settings'];
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
  dtype: string;
  aggregation?: AggFn;
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
