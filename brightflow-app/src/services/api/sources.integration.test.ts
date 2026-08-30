/**
 * Integration tier, `sources` module: the store *catalog* read over the real
 * backend.
 *
 * The committed template (`testdata/workspaces/test`) holds a real
 * `connector:sample/issues` table (40 rows, columns id/title/body plus the
 * `embedding` column topic fitting adds) as a genuine Parquet file. `tableApi.listAvailable` answers `/api/tables` from the store
 * catalog, so this spec proves the frontend can discover the table the template
 * ships — the same seam the app's source picker walks, exercised against a real
 * backend and real storage.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_frontend-backend-integration-tests.md.
 */
import { beforeAll, describe, expect, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { tableApi } from './sources';

describe('table catalog over the committed template', () => {
  beforeAll(() => {
    useIntegrationBackend();
  });

  test('lists the committed connector:sample/issues table with its real rows', async () => {
    const tables = await tableApi.listAvailable();
    expect(tables).toBeTruthy();

    /* One of the template's committed tables, discovered through the genuine
       catalog path — proves the Parquet file surfaces over the API. */
    const issues = tables?.find((t) => t.source_id === 'connector:sample' && t.name === 'issues');
    expect(issues).toBeTruthy();
    expect(issues?.num_rows).toBe(40);
    expect(issues?.num_files).toBe(1);
  });
});
