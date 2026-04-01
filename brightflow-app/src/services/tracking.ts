/**
 * Self-instrumentation: track user actions within the Brightflow app itself.
 * Sends events to the local /api/track and /api/identify endpoints.
 */

const API_BASE = import.meta.env.VITE_API_BASE || '';
const DOMAIN = 'app.brightflow.local';

function post(url: string, data: Record<string, unknown>): void {
  try {
    const body = JSON.stringify(data);
    if (typeof navigator.sendBeacon === 'function') {
      navigator.sendBeacon(`${API_BASE}${url}`, body);
    } else {
      void fetch(`${API_BASE}${url}`, {
        method: 'POST',
        body,
        keepalive: true,
      });
    }
  } catch {
    // Silently ignore tracking errors
  }
}

let currentUserId = '';

/**
 * Identify the current user (call on login).
 */
export function identify(userId: string, traits?: Record<string, unknown>): void {
  currentUserId = userId;
  post('/api/identify', {
    userId,
    domain: DOMAIN,
    traits: traits ?? {},
  });
}

/**
 * Track a named event.
 */
export function track(name: string, props?: Record<string, unknown>): void {
  post('/api/track', {
    name,
    domain: DOMAIN,
    url: window.location.href,
    userId: currentUserId || undefined,
    props: props ?? undefined,
  });
}

/**
 * Clear identity (call on logout).
 */
export function resetIdentity(): void {
  currentUserId = '';
}
