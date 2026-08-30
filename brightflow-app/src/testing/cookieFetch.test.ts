/**
 * Unit coverage for the integration tier's cookie jar.
 *
 * Runs in the unit tier deliberately: the wrapper's contract (capture
 * `Set-Cookie`, replay it as `Cookie`, install exactly once) is pure plumbing
 * over a stubbed `fetch`, so it needs no backend and belongs in the fast loop
 * rather than behind `TEST_INTEGRATION=1`.
 */
import { afterEach, describe, expect, test } from 'vitest';

import { installCookieFetch, resetCookieJar } from './cookieFetch';

/** A `fetch` stub that records sent `Cookie` headers and answers with one. */
function stubFetch(setCookie?: string) {
  const seen: (string | null)[] = [];
  const stub: typeof fetch = (_input, init) => {
    seen.push(new Headers(init?.headers).get('Cookie'));
    const headers = new Headers();
    if (setCookie != null) {
      headers.set('set-cookie', setCookie);
    }
    return Promise.resolve(new Response(null, { headers }));
  };
  return { stub, seen };
}

describe('installCookieFetch', () => {
  const restores: (() => void)[] = [];
  afterEach(() => {
    while (restores.length > 0) {
      restores.pop()?.();
    }
  });

  test('captures Set-Cookie and replays it on the next request', async () => {
    const { stub, seen } = stubFetch('id=abc; Path=/; HttpOnly');
    globalThis.fetch = stub;
    restores.push(installCookieFetch());

    await globalThis.fetch('http://127.0.0.1:1/login');
    await globalThis.fetch('http://127.0.0.1:1/me');

    expect(seen[0]).toBeNull();
    expect(seen[1]).toBe('id=abc');
  });

  test('installing twice wraps once and shares one jar', async () => {
    const { stub, seen } = stubFetch('id=abc; Path=/');
    globalThis.fetch = stub;
    restores.push(installCookieFetch());
    const afterFirst = globalThis.fetch;

    /* The second install must adopt the existing wrapper. Nesting would leave
       two jars setting the same header, with the inner one silently winning. */
    restores.push(installCookieFetch());
    expect(globalThis.fetch).toBe(afterFirst);

    await globalThis.fetch('http://127.0.0.1:1/login');
    await globalThis.fetch('http://127.0.0.1:1/me');
    expect(seen[1]).toBe('id=abc');
  });

  test('a second install restore leaves the wrapper in place', async () => {
    const { stub, seen } = stubFetch('id=abc; Path=/');
    globalThis.fetch = stub;
    const restoreFirst = installCookieFetch();
    const restoreSecond = installCookieFetch();

    restoreSecond();
    await globalThis.fetch('http://127.0.0.1:1/login');
    await globalThis.fetch('http://127.0.0.1:1/me');
    expect(seen[1]).toBe('id=abc');

    restoreFirst();
    expect(globalThis.fetch).toBe(stub);
  });

  test('resetCookieJar drops the session', async () => {
    const { stub, seen } = stubFetch('id=abc; Path=/');
    globalThis.fetch = stub;
    restores.push(installCookieFetch());

    await globalThis.fetch('http://127.0.0.1:1/login');
    resetCookieJar();
    await globalThis.fetch('http://127.0.0.1:1/me');

    expect(seen[1]).toBeNull();
  });
});
