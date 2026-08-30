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
 * Installing is idempotent by construction: the wrapper carries a marker, so a
 * second call adopts the jar already in place instead of wrapping the wrapper.
 * Nesting would leave two jars racing to set the same `Cookie` header, with the
 * innermost silently winning — a bug that only shows once two specs share a
 * worker, which is far too late to find it.
 *
 * Only the integration tier installs this; unit tests never touch it.
 */

/** Marker carried by an installed wrapper, holding the jar it writes into. */
const JAR = Symbol.for('brightflow.cookieFetch.jar');

type CookieJar = Map<string, string>;
type MarkedFetch = typeof fetch & { [JAR]?: CookieJar };

/** The jar of an already-installed wrapper, or `null` if none is installed. */
function installedJar(): CookieJar | null {
  return (globalThis.fetch as MarkedFetch)[JAR] ?? null;
}

/**
 * Install the cookie-aware fetch, returning a `restore()` to undo it.
 *
 * Calling this when a wrapper is already installed is a no-op that returns the
 * existing jar's `restore()`; the caller cannot tell the difference, which is
 * the point.
 */
export function installCookieFetch(): () => void {
  const existing = installedJar();
  if (existing != null) {
    return () => {
      // The first installer owns teardown; a later caller unwinding its own
      // (non-)installation must not rip the wrapper out from under it.
    };
  }

  const original = globalThis.fetch;
  const jar: CookieJar = new Map();

  const wrapped: MarkedFetch = async (input, init) => {
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
  wrapped[JAR] = jar;

  globalThis.fetch = wrapped;
  return () => {
    globalThis.fetch = original;
  };
}

/**
 * Forget every stored cookie, so the next request goes out unauthenticated.
 *
 * Lets a spec drop a session deliberately without uninstalling the wrapper.
 * No-op when nothing is installed.
 */
export function resetCookieJar(): void {
  installedJar()?.clear();
}

/**
 * Put an already-issued cookie into the jar, as if a response had set it.
 *
 * The harness logs in once per run and hands every spec the resulting session
 * this way. Logging in per spec file would be simpler but trips the login rate
 * limiter once there are more than a handful of specs — and a limiter doing its
 * job should not be what caps how many tests the tier can hold.
 *
 * `header` is a raw `Set-Cookie` value; only the name=value pair is kept, which
 * is all the jar replays.
 */
export function seedCookie(header: string): void {
  const jar = installedJar();
  if (jar == null) {
    return;
  }
  const [pair] = header.split(';');
  const eq = pair?.indexOf('=') ?? -1;
  if (pair != null && eq > 0) {
    jar.set(pair.slice(0, eq).trim(), pair.slice(eq + 1).trim());
  }
}
