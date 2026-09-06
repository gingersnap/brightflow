/**
 * Integration tier, `textenrichment` module: Text Explorer over the committed
 * template. `textExploreApi.search` builds its index on demand from the
 * table's real Parquet, needing no artifacts.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_frontend-backend-integration-tests.md.
 */
import { beforeAll, describe, expect, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { textExploreApi } from './textenrichment';

const SOURCE = 'connector:sample';
const TABLE = 'issues';

/** One named include chip; `exclude: false` throughout. */
function include(term: string) {
  return { terms: [{ text: term, exclude: false }], limit: 100, wholeWord: false };
}

describe('Text Explorer over the committed issues table', () => {
  beforeAll(() => {
    useIntegrationBackend();
  });

  test('an empty filter returns every committed row', async () => {
    const res = await textExploreApi.search(SOURCE, TABLE, include(''));
    expect(res?.totalRows).toBe(40);
    expect(res?.matchedRows).toBe(40);
  });

  test('a present term filters to the row that contains it', async () => {
    const res = await textExploreApi.search(SOURCE, TABLE, include('VAT'));
    expect(res?.matchedRows).toBe(1);
    // Row 2 is the only issue mentioning VAT, in both its title and its body.
    const ids = (res?.rows ?? []).map((row) => row.id);
    const titles = (res?.rows ?? []).map((row) => row.title.map((run) => run.t).join(''));
    expect(ids).toEqual(['2']);
    expect(titles).toContain('invoice missing VAT line');
  });

  test('an absent term matches nothing rather than erroring', async () => {
    const res = await textExploreApi.search(SOURCE, TABLE, include('zzz-no-such-term'));
    expect(res?.matchedRows).toBe(0);
  });
});
