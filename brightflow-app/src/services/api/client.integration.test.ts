/**
 * First frontend→backend integration tests: the genuine `services/api` client
 * against the real backend over HTTP (booted by `src/testing/globalSetup.ts`).
 *
 * Below the UI — no browser, no DOM — this proves the seam unit tests cannot:
 * real HTTP transport, a real session (login → cookie replay → protected
 * route), and a real storage round-trip (list + create a source persisted in
 * the workspace). See the testing philosophy in CLAUDE.md and
 * plans/2026-08-29_frontend-backend-integration-tests.md.
 */

import { beforeAll, describe, expect, test } from 'vitest';

import { DEMO_EMAIL, DEMO_PASSWORD, useIntegrationBackend } from '@/testing/withBackend';

import { authApi } from './core';
import { sourceApi } from './sources';

describe('frontend data layer ↔ real backend', () => {
  /* Activate the seam once: point the client at the ephemeral backend and
     install the run's session. This spec then logs in again on purpose — it is
     the one that covers the login round-trip itself. */
  beforeAll(() => {
    useIntegrationBackend();
  });

  test('logs in through the real client and holds a session', async () => {
    const user = await authApi.login(DEMO_EMAIL, DEMO_PASSWORD);
    expect(user?.email).toBe(DEMO_EMAIL);

    // `/api/auth/me` is protected: answering with the user proves the login
    // Cookie was captured and replayed by the wrapper.
    const me = await authApi.me();
    expect(me?.email).toBe(DEMO_EMAIL);
  });

  test('lists and creates a source against real storage', async () => {
    const created = await sourceApi.create('integration.test', 'Integration Source');
    expect(created?.id).toBeTruthy();

    const all = await sourceApi.list();
    expect(all?.some((s) => s.id === created?.id)).toBe(true);
  });
});
