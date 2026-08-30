/**
 * Integration tier, `enrich` module: the versioned-function lifecycle for the
 * built-in ticket kinds.
 *
 * The committed template ships one promoted `ticket_classify` function with
 * two versions, so this spec reads a real function rather than an empty list,
 * and then walks create → edit → delete against the live backend. That
 * exercises the store's read-then-write version transaction and the
 * duplicate-name conflict mapping, neither of which a unit test reaches over
 * HTTP. Creating a function never calls the LLM; only a run does.
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
/** A second table, so the created function cannot collide with the committed one. */
const CRUD_TABLE = 'orders';

describe('ticket functions over the committed template', () => {
  beforeAll(() => {
    useIntegrationBackend();
  });

  test('lists the committed promoted function with its versions', async () => {
    const functions = await enrichFnApi.list(SOURCE, TABLE);
    const seeded = functions?.find((f) => f.name === 'classify');
    expect(seeded, 'template must ship the classify function').toBeTruthy();
    expect(seeded?.kind).toBe('ticket_classify');
    expect(seeded?.status).toBe('promoted');

    /* Two versions: the builder creates the function and then edits its config,
       which is what the store's read-then-write version transaction produces. */
    const versions = await enrichFnApi.versions(seeded?.id ?? '');
    expect(versions?.length).toBe(2);
    expect(seeded?.version).toBe(2);
  });

  test('creating, editing and deleting a function round-trips', async () => {
    const name = `spec_fn_${Date.now()}`;
    const created = await enrichFnApi.create(SOURCE, CRUD_TABLE, {
      name,
      kind: 'ticket_extract',
      config: { text_columns: ['region'], provider_id: 'default' },
    });
    expect(created?.id).toBeTruthy();
    expect(created?.version).toBe(1);
    /* Built-in kinds are promoted from birth: no draft state a sync could wipe. */
    expect(created?.status).toBe('promoted');

    const edited = await enrichFnApi.update(created?.id ?? '', {
      config: { text_columns: ['region'], provider_id: 'default', model: 'x' },
    });
    expect(edited?.version).toBe(2);

    await enrichFnApi.delete(created?.id ?? '', true);
    const after = await enrichFnApi.list(SOURCE, CRUD_TABLE);
    expect(after?.some((f) => f.name === name)).toBe(false);
  });

  test('a duplicate function name is refused, not silently accepted', async () => {
    await expect(
      enrichFnApi.create(SOURCE, TABLE, {
        name: 'classify',
        kind: 'ticket_classify',
        config: { text_columns: ['title'], provider_id: 'default' },
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
