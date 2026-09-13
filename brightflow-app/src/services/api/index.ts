/**
 * Barrel for the REST clients, so `@/services/api` keeps resolving after the
 * split into per-domain files.
 */

export { api, ApiError, authApi } from './core';
export { connectApi } from './connect';
export { actionsApi, agentApi } from './curation';
export { enrichFnApi, llmApi } from './enrich';
export { insightHistoryApi, insightRunsApi, insightsApi } from './insights';
export { productAnalyticsApi } from './productAnalytics';
export { datasetApi, semanticModelApi, semanticsApi, sourceApi, tableApi } from './sources';
export {
  mentionsApi,
  taxonomyApi,
  textExploreApi,
  ticketsApi,
  vocabularyApi,
} from './textenrichment';
export { analyticsApi } from './webAnalytics';
