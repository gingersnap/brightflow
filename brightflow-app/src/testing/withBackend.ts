/**
 * Shared seam-activation for the integration tier.
 *
 * Every `*.integration.test.ts` needs the same preamble: point the real
 * `services/api` client at the ephemeral backend the harness booted, and give
 * Node's cookie-less fetch a jar holding the session. That preamble is exactly
 * the seam unit tests cannot reach, so it is centralised here (not in the
 * bundle) and called once from each spec's `beforeAll`.
 *
 * The session is minted once per run by `globalSetup` and replayed here rather
 * than re-logging-in per file — see the note on the login rate limit there. A
 * spec that wants to exercise logging in should call `authApi.login` itself.
 */
import { inject } from 'vitest';

import { setApiBase } from '@/services/api/core';

import { installCookieFetch, seedCookie } from './cookieFetch';

// The committed template's demo account, created by the workspace builder
// (scripts/build-test-template.sh). Kept in sync with that script by hand.
export const DEMO_EMAIL = 'test@brightflow.local';
export const DEMO_PASSWORD = 'brightflow-test-pass!';

/**
 * Point the real client at the integration backend and install the run's
 * session, so the spec's very first request is already authenticated.
 */
export function useIntegrationBackend(): void {
  setApiBase(inject('apiBase'));
  installCookieFetch();
  seedCookie(inject('sessionCookie'));
}
