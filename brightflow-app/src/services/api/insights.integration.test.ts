/**
 * Integration tier, `insights` module: the run log and the novelty memory.
 *
 * The committed template ships three manual runs (review, trends, drivers) over
 * the `orders` time series, plus the novelty-memory rows a manual run records.
 * So this spec asserts real content on the table that has it, and keeps one
 * deliberate empty-response case on `issues` — a table with no computed runs
 * must answer with a typed empty list, not a 401 or a 500. Both halves are the
 * contract; only one of them is silence.
 *
 * `orders` carries the runs rather than `issues` because insight analysis needs
 * a date column and a measure, which the five-row text fixture has not got.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_review-followup-and-test-env-depth.md.
 */
import { beforeAll, describe, expect, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { insightHistoryApi, insightRunsApi } from './insights';

const SOURCE = 'connector:sample';
const ANALYSED = 'orders';
const UNANALYSED = 'issues';

describe('insight runs over the committed template', () => {
  beforeAll(() => {
    useIntegrationBackend();
  });

  test('the run log returns the seeded review, trends and drivers runs', async () => {
    const runs = await insightRunsApi.list(SOURCE, ANALYSED);
    expect(runs?.length).toBe(3);

    const reportTypes = (runs ?? []).map((r) => r.reportType).toSorted();
    expect(reportTypes).toEqual(['drivers', 'review_weekly', 'trends']);

    // Every seeded run was manual and found something worth logging.
    for (const run of runs ?? []) {
      expect(run.triggeredBy).toBe('manual');
      expect(run.findingCount).toBeGreaterThan(0);
      expect(run.table).toBe(ANALYSED);
    }
  });

  test('the latest-run digest covers the analysed table', async () => {
    const latest = await insightRunsApi.latest(SOURCE);
    expect(latest?.some((r) => r.table === ANALYSED)).toBe(true);
  });

  test('the novelty memory holds the findings those runs showed', async () => {
    const history = await insightHistoryApi.list(SOURCE, ANALYSED);
    expect(history?.length).toBeGreaterThan(0);
  });

  test('a table with no runs answers empty rather than erroring', async () => {
    /* The silence is the contract: a protected, store-backed read over a table
       with nothing computed must still be a typed empty list. */
    expect(await insightRunsApi.list(SOURCE, UNANALYSED)).toEqual([]);
    expect(await insightHistoryApi.list(SOURCE, UNANALYSED)).toEqual([]);
  });
});
