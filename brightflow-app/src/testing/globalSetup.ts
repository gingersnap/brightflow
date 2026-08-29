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
  }
}

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
