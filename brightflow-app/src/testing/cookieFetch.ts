/**
 * Cookie-persistent `fetch` for the integration test tier.
 *
 * The real client sends `credentials: 'include'`, but Node's built-in `fetch`
 * has no cookie jar, so a session set by `Set-Cookie` on login would be lost
 * on the next request. This installs a thin wrapper over `globalThis.fetch`
 * that captures `Set-Cookie` from responses and replays them as `Cookie` on
 * every request — so the integration tests exercise real session auth end to
 * end (see the philosophy in CLAUDE.md and
 * plans/2026-08-29_frontend-backend-integration-tests.md).
 *
 * Only the integration tier installs this; unit tests never touch it.
 */

/** Install the cookie-aware fetch, returning a `restore()` to undo it. */
export function installCookieFetch(): () => void {
  const original = globalThis.fetch;

  // Jar maps cookie name → value for the single host under test.
  const jar = new Map<string, string>();

  const wrapped: typeof fetch = async (input, init) => {
    let href = '';
    if (typeof input === 'string') {
      href = input;
    } else if (input instanceof URL) {
      href = input.href;
    } else {
      href = input.url;
    }
    const url = new URL(href);

    const headers = new Headers(init?.headers);
    const cookie = [...jar.entries()].map(([k, v]) => `${k}=${v}`).join('; ');
    if (cookie) {
      headers.set('Cookie', cookie);
    }

    const response = await original(url, { ...init, headers });

    // Undici merges multiple set-cookie headers; a session login needs one.
    const setCookie = response.headers.get('set-cookie');
    if (setCookie != null) {
      const [pair] = setCookie.split(';');
      if (pair != null) {
        const eq = pair.indexOf('=');
        if (eq > 0) {
          jar.set(pair.slice(0, eq).trim(), pair.slice(eq + 1).trim());
        }
      }
    }

    return response;
  };

  globalThis.fetch = wrapped;
  return () => {
    globalThis.fetch = original;
  };
}
