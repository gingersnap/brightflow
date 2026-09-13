/**
 * Sources, tables, and loaded datasets: listing, CRUD, lazy table loads, and
 * the CSV upload path.
 */

import type { UnifiedSource } from '@/types';
import type { UploadSourceResult } from '@/types/enrichment';
import type {
  DatasetInfo,
  LoadTableResponse,
  QueryResponse,
  SemanticModelImportResponse,
  SemanticModelResponse,
  Source,
} from '@/types/generated';
import type { TableInfo } from '@/types/generated/TableInfo';

import { api, ApiError } from './core';

// Table API - for lazy loading Parquet tables
export const tableApi = {
  // Get list of available tables (metadata only, nothing loaded)
  listAvailable: (): Promise<TableInfo[] | null> => api.get<TableInfo[]>('/api/tables'),
  // Load a specific table into memory
  load: (sourceId: string, name: string): Promise<LoadTableResponse | null> =>
    api.post<LoadTableResponse>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(name)}/load`,
    ),
};

// The source's semantic model: the Ossie document the store rebuilds from
// Its resolved rows, and a pasted document applied as declared rows.
export const semanticModelApi = {
  export: (sourceId: string): Promise<SemanticModelResponse | null> =>
    api.get<SemanticModelResponse>(`/api/sources/${encodeURIComponent(sourceId)}/semantic-model`),
  import: (
    sourceId: string,
    document: unknown,
    dryRun: boolean,
  ): Promise<SemanticModelImportResponse | null> =>
    api.post<SemanticModelImportResponse>(
      `/api/sources/${encodeURIComponent(sourceId)}/semantic-model/import${dryRun ? '?dry_run=true' : ''}`,
      document,
    ),
};

// Dataset-specific API methods (for loaded datasets)
export const datasetApi = {
  delete: (id: string): Promise<unknown> => api.delete(`/api/datasets/${id}`),
  get: (id: string): Promise<DatasetInfo | null> => api.get<DatasetInfo>(`/api/datasets/${id}`),
  list: (): Promise<DatasetInfo[] | null> => api.get<DatasetInfo[]>('/api/datasets'),
  query: (datasetId: string, operations: unknown[]): Promise<QueryResponse | null> =>
    api.post<QueryResponse>('/api/query', { datasetId, operations }),
};

export const sourceApi = {
  list: (): Promise<Source[] | null> => api.get<Source[]>('/api/sources'),
  unifiedList: (): Promise<UnifiedSource[] | null> =>
    api.get<UnifiedSource[]>('/api/sources/unified'),
  create: (domain: string, name: string): Promise<Source | null> =>
    api.post<Source>('/api/sources', { domain, name }),
  get: (id: string): Promise<Source | null> => api.get<Source>(`/api/sources/${id}`),
  update: (id: string, data: { name?: string; timezone?: string }): Promise<Source | null> =>
    api.put<Source>(`/api/sources/${id}`, data),
  delete: (id: string): Promise<unknown> => api.delete(`/api/sources/${id}`),
  snippet: (id: string): Promise<{ snippet: string } | null> =>
    api.get<{ snippet: string }>(`/api/sources/${id}/snippet`),
  /** Persist a CSV as a first-class upload source (survives restarts). */
  uploadCsv: async (file: File, table?: string): Promise<UploadSourceResult> => {
    const formData = new FormData();
    formData.append('file', file);
    if (table != null && table.trim() !== '') {
      formData.append('table', table.trim());
    }
    // Through request() so uploads get the same 401 handling as every call.
    const result = await api.post<UploadSourceResult>('/api/sources/upload', formData);
    if (result == null) {
      throw new ApiError('Upload failed: empty response', 0, null);
    }
    return result;
  },
};
