#!/usr/bin/env node
/**
 * Dev server wrapper that tees Vite/Vite+ stdout+stderr to a size-rotating
 * `logs/frontend.log` (kept at `maxFiles` rotated copies) while still streaming
 * to the terminal. Browser console output is already forwarded to the Vite
 * server stdout via `server.forwardConsole` in vite.config.ts, so this single
 * stream captures both server and client logs.
 *
 * Usage: node scripts/dev-frontend.mjs [extra vp args...]
 */
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { createWriteStream, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..');
const appDir = resolve(__dirname, '..', 'brightflow-app');
const logDir = resolve(repoRoot, 'logs');

// Resolve from the app dir so the dependency in brightflow-app/node_modules is
// found (this script lives at the repo root, outside any node_modules tree).
const require = createRequire(resolve(appDir, 'package.json'));
const rfs = require('rotating-file-stream');

mkdirSync(logDir, { recursive: true });

// Size-rotating stream: rotate at 5 MB, keep 3 rotated copies + the active one.
const logStream = rfs.createStream('frontend.log', {
  size: '5M',
  path: logDir,
  maxFiles: 4,
  compress: false,
});

const ts = () => new Date().toISOString();
const writeLine = (prefix, chunk) => {
  const text = chunk.toString();
  // Tee to the terminal (preserve color) and to the file (plain lines).
  process.stdout.write(text);
  for (const line of text.split('\n')) {
    if (line.length > 0) {
      logStream.write(`${ts()} ${prefix} ${line}\n`);
    }
  }
};

const child = spawn('vp', ['dev', ...process.argv.slice(2)], {
  cwd: appDir,
  stdio: ['inherit', 'pipe', 'pipe'],
  env: process.env,
});

child.stdout.on('data', (d) => writeLine('[vite]', d));
child.stderr.on('data', (d) => writeLine('[vite!]', d));

child.on('exit', (code, signal) => {
  logStream.end(() => {
    if (code !== null) {
      process.exit(code);
    } else {
      process.exit(signal ? 128 : 0);
    }
  });
});

const stop = () => child.kill('SIGTERM');
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
