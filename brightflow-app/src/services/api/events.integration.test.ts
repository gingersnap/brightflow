/**
 * Integration tier, event ingestion and the web-analytics reads over it.
 *
 * The committed template ships an ingest source and a flushed events table, so
 * this is the one spec that walks the whole write path the tracking script
 * uses: POST an event, then read it back through the analytics endpoints that
 * the dashboard calls. That path — buffer, session derivation, flush to
 * Parquet, catalog registration — is the part of the backend the rusqlite
 * conversion touched hardest and the part unit tests cannot reach.
 *
 * The ingest source id is a uuid minted when the template was built, so it is
 * discovered from `/api/sources` rather than hardcoded.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_review-followup-and-test-env-depth.md.
 */
import { beforeAll, describe, expect, inject, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { sourceApi } from './sources';
import { analyticsApi } from './webAnalytics';

/** The domain the template's builder registers (scripts/build-test-template.sh). */
const FIXTURE_DOMAIN = 'fixture.brightflow.local';

let sourceId = '';

describe('event ingestion and analytics over the committed template', () => {
  beforeAll(async () => {
    useIntegrationBackend();
    const sources = await sourceApi.list();
    const fixture = sources?.find((s) => s.domain === FIXTURE_DOMAIN);
    expect(fixture, `template must ship the ${FIXTURE_DOMAIN} source`).toBeTruthy();
    sourceId = fixture?.id ?? '';
  });

  test('accepts a tracked event on the public collection route', async () => {
    /* The public route, posted the way the tracking script posts it — no
       session, no credentials. 202 is the contract: ingestion is buffered, so
       the response cannot promise the event is queryable yet. */
    const response = await fetch(`${inject('apiBase')}/api/event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name: 'pageview',
        url: `https://${FIXTURE_DOMAIN}/spec-probe`,
        domain: FIXTURE_DOMAIN,
        screenWidth: 1280,
      }),
    });
    expect(response.status).toBe(202);
  });

  test('reads dashboard stats over the flushed events', async () => {
    const stats = await analyticsApi.stats(sourceId, '30d');
    expect(stats).toBeTruthy();
    /* The template's seeded events are all written at build time, so the exact
       counts move with the fixture; what is pinned is that the analytics path
       resolves the events table and returns real numbers rather than zeroes. */
    // Counts come back as bigint (ts-rs maps the Rust u64); compare as numbers.
    expect(Number(stats?.pageviews ?? 0)).toBeGreaterThan(0);
    expect(Number(stats?.visitors ?? 0)).toBeGreaterThan(0);
  });

  test('breaks pageviews down by page, referrer and device', async () => {
    const [pages, referrers, devices] = await Promise.all([
      analyticsApi.topPages(sourceId, '30d'),
      analyticsApi.referrers(sourceId, '30d'),
      analyticsApi.devices(sourceId, '30d'),
    ]);

    expect(pages?.length).toBeGreaterThan(0);
    // The builder posts /, /pricing and /docs from one desktop visitor.
    expect(pages?.some((p) => p.name === '/pricing')).toBe(true);
    // One event carries a google referrer.
    expect(referrers?.some((r) => r.name === 'Google')).toBe(true);
    /* `/devices` groups by the `browser` column, not by device type — the route
       name and the data it returns disagree. Asserted as it behaves, not as the
       name suggests, so this spec keeps passing if the naming is ever fixed
       deliberately rather than failing for the wrong reason. */
    expect(devices?.some((d) => d.name === 'Chrome')).toBe(true);
    expect(devices?.some((d) => d.name === 'Mobile Safari')).toBe(true);
  });

  test('the timeseries answers with typed points', async () => {
    const series = await analyticsApi.timeseries(sourceId, '30d');
    expect(Array.isArray(series)).toBe(true);
    expect(series?.length).toBeGreaterThan(0);
  });
});
