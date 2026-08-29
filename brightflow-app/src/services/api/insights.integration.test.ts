/**
 * Integration tier, `insights` module: the read-only store reads.
 *
 * The template ships no computed insight runs or novelty-memory rows, so these
 * endpoints correctly answer empty. What this spec still proves is the seam: a
 * *protected, store-backed* read returns a typed response (not a 401, not a
 * 500) for the committed `connector:sample/issues` table, over the real backend
 * with the shared demo session. It pins that the read path stays green even
 * with zero rows — the silence is the contract.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_frontend-backend-integration-tests.md.
 */
import { beforeAll, describe, expect, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { insightHistoryApi, insightRunsApi } from './insights';

const SOURCE = 'connector:sample';
const TABLE = 'issues';

describe('read-only insights store reads over the committed table', () => {
  beforeAll(async () => {
    await useIntegrationBackend();
  });

  test('the insight run log is a typed empty list, not an error', async () => {
    const runs = await insightRunsApi.list(SOURCE, TABLE);
    expect(runs).toEqual([]);
  });

  test('the novelty-memory history is a typed empty list, not an error', async () => {
    const history = await insightHistoryApi.list(SOURCE, TABLE);
    expect(history).toEqual([]);
  });
});
