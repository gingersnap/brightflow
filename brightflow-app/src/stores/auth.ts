import { defineStore } from 'pinia';
import { ref, computed } from 'vue';

interface AuthUser {
  id: string;
  email: string;
  displayName: string;
  isAdmin: boolean;
}

export type DataMode = 'memory' | 'lazy';

export const useAuthStore = defineStore('auth', () => {
  const user = ref<AuthUser | null>(null);
  const loading = ref(true);
  const error = ref<string | null>(null);
  const dataMode = ref<DataMode>('memory');

  const isAuthenticated = computed(() => user.value !== null);

  async function fetchSettings(): Promise<void> {
    try {
      const { settingsApi } = await import('@/services/api');
      const settings = await settingsApi.get();
      if (settings) {
        dataMode.value = settings.dataMode as DataMode;
      }
    } catch {
      // Default to memory if settings fetch fails
    }
  }

  async function setDataMode(mode: DataMode): Promise<void> {
    const { settingsApi } = await import('@/services/api');
    const result = await settingsApi.update({ dataMode: mode });
    if (result) {
      dataMode.value = result.dataMode as DataMode;
    }
  }

  async function checkAuth(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const { authApi } = await import('@/services/api');
      const result = await authApi.me();
      user.value = result;
      if (result) {
        await fetchSettings();
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
        await fetchSettings();
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Login failed';
      error.value = msg;
      throw e;
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
    dataMode.value = 'memory';
  }

  return {
    user,
    loading,
    error,
    dataMode,
    isAuthenticated,
    checkAuth,
    login,
    logout,
    clearAuth,
    setDataMode,
    fetchSettings,
  };
});
