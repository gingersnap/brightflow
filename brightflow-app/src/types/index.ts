/**
 * Shared type definitions for Brightflow
 */

// Re-export generated types from Rust backend
export type { ColumnInfo, Operation, QueryResponse, WsServerMessage } from './generated';
// Also re-export remaining generated types used across the app
export type {
  DatasetInfo,
  InsightsResponse,
  LoadTableResponse,
  RunTriggerResponse,
  ScheduleResponse,
  SyncRun,
  UnifiedConnector,
  UnifiedJob,
  UnifiedSyncRun,
  UploadResponse,
  User,
} from './generated';

// Filter state (frontend-only)
export interface Filter {
  id: string;
  column: string | null;
  op: string;
  value: unknown;
}

// Aggregation state (frontend-only)
export interface Aggregation {
  id: string;
  column: string;
  function: string;
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
  agg: string;
}

// Pivot field (frontend-only)
export interface PivotField {
  id: string;
  column: string;
  dtype: string;
  aggregation?: string;
}

// Frontend-only pivot operation (extends wire format with UI-only fields)
export interface PivotOperation {
  type: 'pivot';
  index: string[];
  columns: string | null;
  values: string | Array<{ column: string; agg: string }>;
  agg: string | string[];
  includeSubtotals?: boolean;
  includeTotals?: boolean;
}

// Connection status (frontend-only)
export type ConnectionStatus = 'connected' | 'connecting' | 'disconnected' | 'error';

// View modes (frontend-only)
export type ViewMode = 'table' | 'pivot' | 'chart' | 'split' | 'number';
export type ChartType = 'bar' | 'line' | 'pie' | 'scatter';

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
