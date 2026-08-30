/**
 * Enrichment-function types: deliberate narrowings of the generated ts-rs
 * shapes (EnrichFunctionResponse, EnrichRunResponse, SampleRunResponse, …).
 *
 * The wire types carry `kind: string` / `status: string`; these refine them to
 * the closed unions the UI switches on. Purely identical shapes re-export the
 * generated type instead of restating it. When a wire shape changes, the
 * pre-commit regen breaks the narrowed type here — fix it to match.
 */

export type FunctionKind = 'ticket_classify' | 'ticket_extract';
/** Built-in kinds are promoted from birth; the status stays for the wire shape. */
export type FunctionStatus = 'draft' | 'promoted';

/**
 * Config payload shared by the two built-in ticket kinds. The vocabulary
 * snapshot is server-injected and never sent by the client.
 */
export interface TicketFunctionConfig {
  text_columns: string[];
  language_column: string | null;
  provider_id: string;
  model: string | null;
}

export interface EnrichFunction {
  id: string;
  name: string;
  kind: FunctionKind;
  status: FunctionStatus;
  version: number;
  config: unknown;
  staleRowCount: number | null;
  activeRunId?: string | null;
  createdAt: string;
  updatedAt: string;
}

export type { FunctionVersionResponse as FunctionVersion } from './generated';

export interface SampleCell {
  rowKey: string;
  inputs: Record<string, string>;
  value?: Record<string, unknown> | null;
  status: 'ok' | 'error';
  error?: string | null;
  cached: boolean;
}

export interface SampleRunResult {
  rows: SampleCell[];
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  cachedTokens: number;
  cacheHits: number;
}

export type RunScope = 'missing' | 'all' | 'failed';

export interface EnrichEstimate {
  rowsTotal: number;
  rowsUncached: number;
  rowsToRun: number;
  p75TokensPerRow: number | null;
  estimatedTokens: number;
  basis: 'history' | 'heuristic';
}

export interface EnrichRun {
  id: string;
  functionId: string;
  version: number;
  mode: 'sample' | 'full' | 'incremental';
  status: 'running' | 'completed' | 'failed' | 'cancelled';
  rowsTotal: number;
  rowsDone: number;
  rowsFailed: number;
  rowsCached: number;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  /** Prompt tokens the provider served from its prefix cache. */
  cachedTokens: number;
  error?: string | null;
  createdAt: string;
  finishedAt?: string | null;
}

export type { UploadSourceResponse as UploadSourceResult } from './generated';
