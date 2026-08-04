/**
 * REST plumbing shared by every domain client: base URL, JSON/FormData
 * handling, the ApiError shape, and the global 401 hook that logs the whole
 * UI out at once. The auth endpoints live here too — they are what the 401
 * handling is defined against.
 */

import type { User } from '@/types/generated';

const API_BASE = import.meta.env.VITE_API_BASE ?? '';

export class ApiError extends Error {
  status: number;
  data: unknown;

  constructor(message: string, status: number, data: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.data = data;
  }
}

interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: unknown;
}

async function request<T>(endpoint: string, options: RequestOptions = {}): Promise<T | null> {
  const url = `${API_BASE}${endpoint}`;

  const { body, ...restOptions } = options;
  // FormData sets its own multipart Content-Type (with boundary); forcing
  // JSON content-type on it would corrupt the upload.
  const isForm = body instanceof FormData;
  const headers: Record<string, string> = isForm ? {} : { 'Content-Type': 'application/json' };
  if (
    restOptions.headers != null &&
    typeof restOptions.headers === 'object' &&
    !Array.isArray(restOptions.headers) &&
    !(restOptions.headers instanceof Headers)
  ) {
    Object.assign(headers, restOptions.headers);
  }
  const config: RequestInit = {
    credentials: 'include',
    headers,
    ...restOptions,
  };

  if (isForm) {
    config.body = body;
  } else if (body != null && typeof body === 'object') {
    config.body = JSON.stringify(body);
  }

  const response = await fetch(url, config);

  if (!response.ok) {
    // Handle 401 for non-auth endpoints: clear auth state
    if (response.status === 401 && !endpoint.startsWith('/api/auth/')) {
      // Clear the cached session so the whole UI logs out at once.
      const [{ useQueryCache }, { AUTH_SESSION_KEY }, { resetIdentity }] = await Promise.all([
        import('@pinia/colada'),
        import('@/composables/useAuth'),
        import('@/services/tracking'),
      ]);
      useQueryCache().setQueryData(AUTH_SESSION_KEY, null);
      resetIdentity();
    }
    // oxlint-disable-next-line @typescript-eslint/no-unsafe-assignment -- JSON boundary
    const data: { message?: string; error?: { message?: string } } = await response
      .json()
      .catch(() => ({}));
    const message = data.error?.message ?? data.message ?? `Request failed: ${response.status}`;
    throw new ApiError(message, response.status, data);
  }

  // Handle empty responses
  const text = await response.text();
  // oxlint-disable-next-line @typescript-eslint/no-unsafe-return -- JSON boundary: parse returns any
  return text ? JSON.parse(text) : null;
}

export const api = {
  delete: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'DELETE', ...options }),
  get: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'GET', ...options }),
  post: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'POST', body, ...options }),
  put: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'PUT', body, ...options }),
};

// Auth API
export const authApi = {
  login: (email: string, password: string): Promise<User | null> =>
    api.post<User>('/api/auth/login', { email, password }),
  logout: (): Promise<unknown> => api.post('/api/auth/logout'),
  me: (): Promise<User | null> => api.get<User>('/api/auth/me'),
};
