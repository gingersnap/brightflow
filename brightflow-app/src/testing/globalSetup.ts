/**
 * Boots the real backend for the integration test tier and tears it down after.
 *
 * One shared backend per run: `globalSetup` copies the committed test workspace
 * into a throwaway dir, builds and spawns the `brightflow-api` `test-server` bin
 * against it (ephemeral port), and hands the bound base URL to tests via Vitest
 * `provide`/`inject`. Teardown kills the server and removes the workspace copy.
 * The unit tier is unaffected — this file only loads under the integration
 * project (see the `test:` block of `vite.config.ts`).
 *
 * This is the "minimal real server" from the plan: no scheduler/WebSocket —
 * the harness only needs the HTTP + storage path.
 */

import { execSync, spawn, type ChildProcess } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import type { TestProject } from 'vitest/node';

declare module 'vitest' {
  interface ProvidedContext {
    /** Base URL of the ephemeral test backend, e.g. `http://127.0.0.1:37453`. */
    apiBase: string;
    /** `Set-Cookie` value for the one session this run logs in with. */
    sessionCookie: string;
  }
}

/** The committed template's demo account (scripts/build-test-template.sh). */
const DEMO_EMAIL = 'test@brightflow.local';
const DEMO_PASSWORD = 'brightflow-test-pass!';

let server: ChildProcess | null = null;
let wsDir: string | null = null;

export async function setup(project: TestProject): Promise<void> {
  const appDir = process.cwd();
  const repoRoot = resolve(appDir, '..');
  const template = join(repoRoot, 'testdata', 'workspaces');
  if (!existsSync(template)) {
    throw new Error(
      `integration tier needs the committed test workspace at ${template}; ` +
        'run scripts/test-env.sh setup or rebuild it per plans/2026-08-29_test-workspace-architecture.md',
    );
  }

  // Fresh workspace copy: one throwaway data dir per run.
  wsDir = mkdtempSync(join(tmpdir(), 'bf-integration-'));
  cpSync(template, join(wsDir, 'workspaces'), { recursive: true });

  // Ensure the test-server binary is current (incremental — fast once built).
  execSync('cargo build -p brightflow-api --bin test-server', {
    cwd: repoRoot,
    stdio: 'inherit',
  });

  const child = spawn(join(repoRoot, 'target', 'debug', 'test-server'), [], {
    env: {
      ...process.env,
      BRIGHTFLOW_DATA_DIR: wsDir,
      BRIGHTFLOW_WORKSPACE: 'test',
      BRIGHTFLOW_TEST_HOST: '127.0.0.1',
      BRIGHTFLOW_TEST_PORT: '0',
    },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  server = child;

  const apiBase = await new Promise<string>((resolvePort, reject) => {
    let stdout = '';
    child.stdout?.on('data', (chunk: Buffer) => {
      stdout += chunk.toString();
      const match = /TEST_SERVER_LISTENING (?<addr>127\.0\.0\.1:\d+)/u.exec(stdout);
      const addr = match?.groups?.['addr'];
      if (addr != null && addr !== '') {
        resolvePort(`http://${addr}`);
      }
    });
    child.on('error', (err) => {
      reject(err);
    });
    child.on('exit', (code) => {
      reject(new Error(`test-server exited early (code ${code})`));
    });
  });

  project.provide('apiBase', apiBase);

  /* One login for the whole run. The login endpoint is rate limited (a burst of
     five, then one per twelve seconds), so a login per spec file would start
     failing as soon as the tier grew past a handful of specs — the limiter
     working correctly is not a reason to cap how many tests can exist. Specs
     replay this session instead; the one spec that exercises logging in does
     its own, which stays well inside the burst. */
  const login = await fetch(`${apiBase}/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: DEMO_EMAIL, password: DEMO_PASSWORD }),
  });
  if (!login.ok) {
    throw new Error(
      `harness login failed (${login.status}); the template must ship the ${DEMO_EMAIL} account`,
    );
  }
  const cookie = login.headers.get('set-cookie');
  if (cookie == null) {
    throw new Error('harness login returned no Set-Cookie; sessions are not being issued');
  }
  project.provide('sessionCookie', cookie);
}

export function teardown(): void {
  if (server != null) {
    server.kill();
  }
  server = null;
  if (wsDir != null) {
    rmSync(wsDir, { recursive: true, force: true });
  }
  wsDir = null;
}
