/**
 * Integration tier, `topics` module: Text Explorer over the committed template.
 *
 * Two read paths, both against committed data. `textExploreApi.search` builds
 * its index on demand from the table's real Parquet, needing no artifacts.
 * `topicsApi.overview` reads the cluster artifacts the template now ships —
 * fitted at build time by `scripts/build-test-template.sh`, which is the only
 * step that needs the embedding model; reads load the artifacts from disk and
 * never embed, so this spec runs without it.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_frontend-backend-integration-tests.md.
 */
import { beforeAll, describe, expect, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { textExploreApi, topicsApi } from './topics';

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

  test('the topics overview is ready and reports the fitted clusters', async () => {
    const overview = await topicsApi.overview(SOURCE, TABLE);
    /* `ready: false` is how the endpoint reports missing or stale artifacts,
       so this also pins that the committed artifacts still match the engine's
       artifact version — a refit-needed answer here means the template is due
       a rebuild, not that the endpoint is broken. */
    expect(overview?.ready, `overview not ready: ${overview?.reason ?? ''}`).toBe(true);
    expect(overview?.totalRows).toBe(40);
    expect(overview?.k).toBe(2);
    expect(overview?.embeddingModelId).toContain('potion');
    /* The seed is two deliberately distinct themes (billing and performance),
       so both clusters clear the server's min-size floor and neither is
       hidden. A 5-row fixture could never show a cluster at all. */
    expect(overview?.clusters.length).toBe(2);
    expect(overview?.hiddenClusters).toBe(0);
    for (const cluster of overview?.clusters ?? []) {
      expect(cluster.size).toBeGreaterThan(0);
      expect(cluster.topTerms.length).toBeGreaterThan(0);
    }
  });
});
