/**
 * Fires insight analysis runs (review/trends/drivers) and feeds the results
 * into the insights store.
 *
 * Runs are commands, not cacheable reads — a run recomputes the report
 * server-side — so they are plain calls rather than colada queries; the store
 * owns the resulting tree as client state merged with the curation overlay.
 */

import { insightsApi } from '@/services/api';
import { type Cadence, useInsightsStore } from '@/stores/insights';

export function useInsightsRuns() {
  const store = useInsightsStore();

  async function runReview(selectedCadence?: Cadence): Promise<void> {
    const cadence = selectedCadence ?? store.cadence;
    if (!store.beginRun('review', cadence)) {
      return;
    }
    try {
      const result = await insightsApi.runReview({
        cadence,
        datasetId: store.selectedTable ?? '',
        sourceId: store.selectedSourceId ?? '',
      });
      store.completeRun(result);
    } catch (error) {
      store.failRun(error instanceof Error ? error.message : 'Analysis failed');
    }
  }

  async function runTrends(): Promise<void> {
    if (!store.beginRun('trends')) {
      return;
    }
    try {
      const result = await insightsApi.runTrends({
        datasetId: store.selectedTable ?? '',
        sourceId: store.selectedSourceId ?? '',
      });
      store.completeRun(result);
    } catch (error) {
      store.failRun(error instanceof Error ? error.message : 'Analysis failed');
    }
  }

  async function runDrivers(): Promise<void> {
    if (!store.beginRun('drivers')) {
      return;
    }
    try {
      const result = await insightsApi.runDrivers({
        datasetId: store.selectedTable ?? '',
        sourceId: store.selectedSourceId ?? '',
      });
      store.completeRun(result);
    } catch (error) {
      store.failRun(error instanceof Error ? error.message : 'Analysis failed');
    }
  }

  return { runDrivers, runReview, runTrends };
}
