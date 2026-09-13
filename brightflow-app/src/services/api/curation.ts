/**
 * First-class curation actions and agent runs: one dispatch path for humans
 * and agents.
 */

import type {
  Action,
  ActionLogEntry,
  ActionManifestEntry,
  ActionResponse,
  AgentRunResponse,
  BulkApproveResponse,
  BulkRejectResponse,
  BulkUndoResponse,
  PendingCount,
} from '@/types/generated';

import { api } from './core';

// First-class curation actions: one dispatch path for humans and agents
export const actionsApi = {
  dispatch: (action: Action, requestId: string): Promise<ActionResponse | null> =>
    api.post<ActionResponse>('/api/actions', { action, requestId }),
  /** Action catalog: kind, label, description, undoability, param schema. */
  manifest: (): Promise<ActionManifestEntry[] | null> =>
    api.get<ActionManifestEntry[]>('/api/actions/manifest'),
  feed: (limit = 100): Promise<ActionLogEntry[] | null> =>
    api.get<ActionLogEntry[]>(`/api/actions?limit=${limit}`),
  approve: (id: number): Promise<ActionResponse | null> =>
    api.post<ActionResponse>(`/api/actions/${id}/approve`),
  /** Approve every pending proposal, oldest first. */
  approveAll: (): Promise<BulkApproveResponse | null> =>
    api.post<BulkApproveResponse>('/api/actions/approve-all'),
  /** True pending count — the feed is truncated, so don't count it client-side. */
  pendingCount: (): Promise<PendingCount | null> =>
    api.get<PendingCount>('/api/actions/pending-count'),
  reject: (id: number): Promise<ActionResponse | null> =>
    api.post<ActionResponse>(`/api/actions/${id}/reject`),
  undo: (id: number): Promise<ActionResponse | null> =>
    api.post<ActionResponse>(`/api/actions/${id}/undo`),
};

// Agent runs: LLM curation through the same action layer
export const agentApi = {
  start: (req: {
    kind: string;
    sourceId: string;
    table: string;
    /** "auto_apply" (default) or "propose". */
    mode?: 'auto_apply' | 'propose';
    /** Parent category for `propose_subcategories`. */
    parentId?: number;
  }): Promise<AgentRunResponse | null> => api.post<AgentRunResponse>('/api/agent/runs', req),
  /** Revert every applied, undoable action of a run, newest first. */
  undoAll: (id: number): Promise<BulkUndoResponse | null> =>
    api.post<BulkUndoResponse>(`/api/agent/runs/${id}/undo-all`),
  /** Apply every pending proposal of a run, oldest first. */
  approveAll: (id: number): Promise<BulkApproveResponse | null> =>
    api.post<BulkApproveResponse>(`/api/agent/runs/${id}/approve-all`),
  /** Reject every pending proposal of a run. */
  rejectAll: (id: number): Promise<BulkRejectResponse | null> =>
    api.post<BulkRejectResponse>(`/api/agent/runs/${id}/reject-all`),
  list: (limit = 50): Promise<AgentRunResponse[] | null> =>
    api.get<AgentRunResponse[]>(`/api/agent/runs?limit=${limit}`),
  get: (id: number): Promise<AgentRunResponse | null> =>
    api.get<AgentRunResponse>(`/api/agent/runs/${id}`),
  cancel: (id: number): Promise<AgentRunResponse | null> =>
    api.post<AgentRunResponse>(`/api/agent/runs/${id}/cancel`),
};
