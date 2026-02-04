/**
 * Shared type definitions for Brightflow
 */

// Column metadata from backend
export interface Column {
  name: string
  dtype: string
}

// Filter state
export interface Filter {
  id: string
  column: string | null
  op: string
  value: unknown
}

// Aggregation state
export interface Aggregation {
  id: string
  column: string
  function: string
  alias: string
}

// Query section state
export interface SectionState {
  enabled: boolean
  collapsed: boolean
}

export interface QuerySections {
  filter: SectionState
  select: SectionState
  groupBy: SectionState
  pivot: SectionState
  sort: SectionState
  limit: SectionState
}

// Pivot state
export interface PivotState {
  index: string[]
  columns: string | null
  values: string | null
  agg: string
}

// Pivot field
export interface PivotField {
  id: string
  column: string
  dtype: string
  aggregation?: string
}

// Format rule for conditional formatting
export interface FormatRule {
  id: string
  [key: string]: unknown
}

// Query operations
export interface FilterOperation {
  type: 'filter'
  column: string
  op: string
  value?: unknown
}

export interface GroupByOperation {
  type: 'groupBy'
  by: string[]
  aggs: Array<{
    column: string
    function: string
    alias: string
  }>
}

export interface PivotOperation {
  type: 'pivot'
  index: string[]
  columns: string | null
  values: string | Array<{ column: string; agg: string }>
  agg: string | string[]
  includeSubtotals?: boolean
  includeTotals?: boolean
}

export interface SelectOperation {
  type: 'select'
  columns: string[]
}

export interface SortOperation {
  type: 'sort'
  by: string
  descending: boolean
}

export interface LimitOperation {
  type: 'limit'
  n: number
}

export type QueryOperation =
  | FilterOperation
  | GroupByOperation
  | PivotOperation
  | SelectOperation
  | SortOperation
  | LimitOperation

// WebSocket message types
export interface WsMessage {
  type: string
  [key: string]: unknown
}

export interface ConnectedMessage extends WsMessage {
  type: 'connected'
  serverVersion: string
}

export interface MetadataMessage extends WsMessage {
  type: 'metadata'
  dataset_id: string
  name: string
  row_count: number
  columns: Column[]
}

export interface QueryResultMessage extends WsMessage {
  type: 'queryResult'
  columns: Column[]
  rows: unknown[][]
  row_count: number
  total_rows: number
  execution_time_ms: number
}

export interface ErrorMessage extends WsMessage {
  type: 'error'
  message: string
}

export interface DatasetListMessage extends WsMessage {
  type: 'datasetList'
  datasets: Array<{
    id: string
    name: string
  }>
}

// Connection status
export type ConnectionStatus = 'connected' | 'connecting' | 'disconnected' | 'error'

// View modes
export type ViewMode = 'table' | 'pivot' | 'chart' | 'split' | 'number'
export type ChartType = 'bar' | 'line' | 'pie' | 'scatter'

// Operator definition
export interface OperatorDef {
  label: string
  types: string[]
  noValue?: boolean
  isArray?: boolean
}

export interface Operator {
  value: string
  label: string
  noValue: boolean
  isArray: boolean
}

// Aggregation definition
export interface AggregationDef {
  label: string
  description: string
  types: string[]
  usesStar?: boolean
}

export interface AggregationOption {
  value: string
  label: string
  description: string
  usesStar?: boolean
}
