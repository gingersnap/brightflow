import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { identify, resetIdentity, track } from '@/services/tracking';
import type { User } from '@/types';

export const useAuthStore = defineStore('auth', () => {
  const user = ref<User | null>(null);
  const loading = ref(true);
  const error = ref<string | null>(null);

  const isAuthenticated = computed(() => user.value !== null);

  async function checkAuth(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const { authApi } = await import('@/services/api');
      const result = await authApi.me();
      user.value = result;
      if (result) {
        identify(result.email, { name: result.displayName });
      }
    } catch {
      user.value = null;
    } finally {
      loading.value = false;
    }
  }

  async function login(email: string, password: string): Promise<void> {
    error.value = null;
    const { authApi } = await import('@/services/api');
    try {
      const result = await authApi.login(email, password);
      user.value = result;
      if (result) {
        identify(result.email, { name: result.displayName });
        track('login');
      }
      // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the store ref
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Login failed';
      error.value = msg;
      throw err;
    }
  }

  async function logout(): Promise<void> {
    const { authApi } = await import('@/services/api');
    try {
      await authApi.logout();
    } catch {
      // Ignore logout errors
    }
    clearAuth();
  }

  function clearAuth(): void {
    user.value = null;
    resetIdentity();
  }

  return {
    checkAuth,
    clearAuth,
    error,
    isAuthenticated,
    loading,
    login,
    logout,
    user,
  };
});
