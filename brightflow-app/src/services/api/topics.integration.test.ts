/**
 * Integration tier, `topics` module: Text Explorer over the committed template.
 *
 * `textExploreApi.search` builds its index on demand from the table's real
 * Parquet and needs no trained ML artifacts — unlike `topicsApi.overview`, which
 * reads precomputed embeddings/labels the template does not ship. So this spec
 * covers exactly the part of the module that works off committed data alone: a
 * real read path over the 5-row `connector:sample/issues` table (id/title/body),
 * proving the on-demand no-ML path end to end over the wire.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_frontend-backend-integration-tests.md.
 */
import { beforeAll, describe, expect, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { textExploreApi } from './topics';

const SOURCE = 'connector:sample';
const TABLE = 'issues';

/** One named include chip; `exclude: false` throughout. */
function include(term: string) {
  return { terms: [{ text: term, exclude: false }], limit: 100, wholeWord: false };
}

describe('Text Explorer over the committed issues table', () => {
  beforeAll(async () => {
    await useIntegrationBackend();
  });

  test('an empty filter returns every committed row', async () => {
    const res = await textExploreApi.search(SOURCE, TABLE, include(''));
    expect(res?.totalRows).toBe(5);
    expect(res?.matchedRows).toBe(5);
  });

  test('a present term filters to the row that contains it', async () => {
    const res = await textExploreApi.search(SOURCE, TABLE, include('delta'));
    expect(res?.matchedRows).toBe(1);
    // Row 4's title is "delta" and its snippet body holds "delta drift".
    const ids = (res?.rows ?? []).map((row) => row.id);
    const titles = (res?.rows ?? []).map((row) => row.title.map((run) => run.t).join(''));
    expect(ids).toEqual(['4']);
    expect(titles).toContain('delta');
  });

  test('an absent term matches nothing rather than erroring', async () => {
    const res = await textExploreApi.search(SOURCE, TABLE, include('zzz-no-such-term'));
    expect(res?.matchedRows).toBe(0);
  });
});
