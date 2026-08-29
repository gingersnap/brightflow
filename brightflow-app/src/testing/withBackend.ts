/**
 * Shared seam-activation for the integration tier.
 *
 * Every `*.integration.test.ts` needs the same preamble: point the real
 * `services/api` client at the ephemeral backend the harness booted, give
 * Node's cookie-less fetch a jar so the session survives, and log in as the
 * committed demo user. That preamble is exactly the seam unit tests cannot
 * reach, so it is centralised here (not in the bundle) and called once from
 * each spec's `beforeAll`.
 */
import { inject } from 'vitest';

import { authApi, setApiBase } from '@/services/api/core';
import type { User } from '@/types/generated';

import { installCookieFetch } from './cookieFetch';

// The committed template's demo account, planted by the workspace builder in
// Scripts/test-env.sh (see testdata/workspaces/test/auth.db).
export const DEMO_EMAIL = 'test@brightflow.local';
export const DEMO_PASSWORD = 'brightflow-test-pass!';

/**
 * Point the real client at the integration backend, install the cookie jar,
 * and (as the committed demo user) open a real session for the spec.
 */
export function useIntegrationBackend(): Promise<User | null> {
  setApiBase(inject('apiBase'));
  installCookieFetch();
  return authApi.login(DEMO_EMAIL, DEMO_PASSWORD);
}
