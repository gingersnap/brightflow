/**
 * Enrichment-function types: deliberate narrowings of the generated ts-rs
 * shapes (EnrichFunctionResponse, EnrichRunResponse, SampleRunResponse, …).
 *
 * The wire types carry `kind: string` / `status: string`; these refine them to
 * the closed unions the UI switches on. Purely identical shapes re-export the
 * generated type instead of restating it. When a wire shape changes, the
 * pre-commit regen breaks the narrowed type here — fix it to match.
 */

export type FunctionKind = 'llm_prompt' | 'topic_model' | 'classifier';
export type FunctionStatus = 'draft' | 'promoted';

export type OutputType =
  | { type: 'string' }
  | { type: 'number' }
  | { type: 'bool' }
  | { type: 'json' }
  | { type: 'enum'; values: string[] };

export interface OutputField {
  name: string;
  dtype: OutputType;
  description: string;
}

/** Llm_prompt config payload (the `kind` tag is added server-side). */
export interface LlmPromptConfig {
  input_columns: string[];
  prompt_template: string;
  outputs: OutputField[];
  provider_id: string;
  model: string | null;
}

/** Topic_model config payload — optional overrides over builtin defaults. */
export interface TopicModelConfig {
  text_columns: string[] | null;
  cleaning_profile: string | null;
  language_column: string | null;
  embedder: string | null;
  min_cluster_size: number | null;
  algorithm: string | null;
}

export type FunctionConfig = LlmPromptConfig | TopicModelConfig | Record<string, never>;

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
  error?: string | null;
  createdAt: string;
  finishedAt?: string | null;
}

export type { UploadSourceResponse as UploadSourceResult } from './generated';
