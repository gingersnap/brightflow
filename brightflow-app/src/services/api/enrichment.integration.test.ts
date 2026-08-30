/**
 * Integration tier, `enrich` module: the versioned-function lifecycle.
 *
 * The committed template ships one promoted `topic_model` function with two
 * versions, so this spec reads a real function rather than an empty list, and
 * then walks create → edit → promote against the live backend. That exercises
 * the store's read-then-write version transaction and the duplicate-name
 * conflict mapping, neither of which a unit test reaches over HTTP.
 *
 * See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_review-followup-and-test-env-depth.md.
 */
import { beforeAll, describe, expect, test } from 'vitest';

import { useIntegrationBackend } from '@/testing/withBackend';

import { ApiError } from './core';
import { enrichFnApi } from './enrich';

const SOURCE = 'connector:sample';
const TABLE = 'issues';
/** A table with no topic model yet — only one per table is allowed. */
const CRUD_TABLE = 'orders';

describe('enrichment functions over the committed template', () => {
  beforeAll(() => {
    useIntegrationBackend();
  });

  test('lists the committed promoted function with its versions', async () => {
    const functions = await enrichFnApi.list(SOURCE, TABLE);
    const seeded = functions?.find((f) => f.name === 'issue_topics');
    expect(seeded, 'template must ship the issue_topics function').toBeTruthy();
    expect(seeded?.kind).toBe('topic_model');
    expect(seeded?.status).toBe('promoted');

    /* Two versions: the builder creates the function and then edits its config,
       which is what the store's read-then-write version transaction produces. */
    const versions = await enrichFnApi.versions(seeded?.id ?? '');
    expect(versions?.length).toBe(2);
    expect(seeded?.version).toBe(2);
  });

  test('creating, editing and promoting a function round-trips', async () => {
    /* On `orders`, not `issues`: a table may hold only one topic-model
       function, and the template already ships one on `issues`. */
    const name = `spec_fn_${Date.now()}`;
    const created = await enrichFnApi.create(SOURCE, CRUD_TABLE, {
      name,
      kind: 'topic_model',
      config: { algorithm: 'kmeans' },
    });
    expect(created?.id).toBeTruthy();
    expect(created?.version).toBe(1);

    const edited = await enrichFnApi.update(created?.id ?? '', {
      config: { algorithm: 'kmeans', cleaning_profile: 'plain' },
    });
    expect(edited?.version).toBe(2);

    const promoted = await enrichFnApi.promote(created?.id ?? '');
    expect(promoted?.status).toBe('promoted');

    await enrichFnApi.delete(created?.id ?? '', true);
    const after = await enrichFnApi.list(SOURCE, CRUD_TABLE);
    expect(after?.some((f) => f.name === name)).toBe(false);
  });

  test('a duplicate function name is refused, not silently accepted', async () => {
    await expect(
      enrichFnApi.create(SOURCE, TABLE, {
        name: 'issue_topics',
        kind: 'topic_model',
        config: { algorithm: 'kmeans' },
      }),
    ).rejects.toBeInstanceOf(ApiError);
  });

  test('an unknown function kind is rejected by the server', async () => {
    await expect(
      enrichFnApi.create(SOURCE, CRUD_TABLE, {
        name: 'spec_bad_kind',
        kind: 'not_a_kind',
        config: {},
      }),
    ).rejects.toBeInstanceOf(ApiError);
  });
});
