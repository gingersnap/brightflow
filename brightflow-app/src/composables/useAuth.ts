/**
 * Session state for the logged-in user, on the colada cache.
 *
 * The session query starts pending on a cold load because the session is
 * unknown until `/api/auth/me` resolves — defaulting to "logged out" would
 * flash the login screen at users who are in fact signed in. Callers gate on
 * `loading` for exactly that reason.
 *
 * The 401 interceptor in `services/api` clears the cached session via
 * `clearAuthSession` so an expired cookie logs the UI out everywhere at once.
 */

import { useMutation, useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';

import { authApi } from '@/services/api';
import { identify, resetIdentity, track } from '@/services/tracking';
import type { User } from '@/types';

export const AUTH_SESSION_KEY = ['auth-session'];

export function useAuth() {
  const queryCache = useQueryCache();

  const sessionQuery = useQuery({
    key: AUTH_SESSION_KEY,
    query: async (): Promise<User | null> => {
      const result = await authApi.me().catch(() => null);
      if (result) {
        identify(result.email, { name: result.displayName });
      }
      return result;
    },
  });

  const user = computed(() => sessionQuery.data.value ?? null);
  const isAuthenticated = computed(() => user.value != null);
  // Pending = cold load, session unknown; never true again after first resolve.
  const loading = computed(() => sessionQuery.isPending.value);

  const loginError = ref<string | null>(null);

  const loginMutation = useMutation({
    mutation: async ({ email, password }: { email: string; password: string }) => {
      loginError.value = null;
      const result = await authApi.login(email, password);
      if (result) {
        identify(result.email, { name: result.displayName });
        track('login');
      }
      return result;
    },
    onSuccess: (result) => {
      queryCache.setQueryData(AUTH_SESSION_KEY, result);
    },
    onError: (error) => {
      loginError.value = error instanceof Error ? error.message : 'Login failed';
    },
  });

  async function login(email: string, password: string): Promise<void> {
    await loginMutation.mutateAsync({ email, password });
  }

  async function logout(): Promise<void> {
    await authApi.logout().catch(() => null);
    queryCache.setQueryData(AUTH_SESSION_KEY, null);
    resetIdentity();
  }

  return {
    isAuthenticated,
    loading,
    login,
    loginError,
    logout,
    user,
  };
}
