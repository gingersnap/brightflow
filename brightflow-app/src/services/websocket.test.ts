/**
 * Unit tests for the WebSocketClient's pure-ish logic: the backoff schedule,
 * the on()/unsubscribe contract, and the disconnect/reconnect contract.
 *
 * Deliberately out of scope: real sockets and heartbeat network behavior —
 * WebSocket is replaced with a minimal fake and timers are faked, so these
 * tests pin the client's decisions, not the transport.
 */

import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

import { WebSocketClient } from './websocket';

/** Minimal WebSocket stand-in the client can drive. */
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static OPEN = 1;
  static CONNECTING = 0;

  readyState = FakeWebSocket.CONNECTING;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onclose: ((event: { code: number; reason: string; wasClean: boolean }) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;

  readonly url: string;

  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
  }

  open(): void {
    this.readyState = FakeWebSocket.OPEN;
    this.onopen?.();
  }

  /** Simulate the server/network closing the socket (unclean by default). */
  dropped(): void {
    this.readyState = 3;
    this.onclose?.({ code: 1006, reason: 'dropped', wasClean: false });
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(code: number, reason: string): void {
    this.readyState = 3;
    // A client-initiated close arrives as a clean close event.
    this.onclose?.({ code, reason, wasClean: true });
  }
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeWebSocket.instances = [];
  vi.stubGlobal('WebSocket', FakeWebSocket);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('backoff schedule', () => {
  test('doubles per attempt and caps at reconnectDelayMax', () => {
    const client = new WebSocketClient('ws://test', {
      reconnectDelay: 1000,
      reconnectDelayMax: 4000,
      reconnectAttempts: 10,
    });
    client.connect();
    expect(FakeWebSocket.instances).toHaveLength(1);

    // Each dropped connection schedules the next attempt — 1000, 2000, 4000ms —
    // Capped at 4000.
    const expectedDelays = [1000, 2000, 4000, 4000];
    for (const [i, delay] of expectedDelays.entries()) {
      FakeWebSocket.instances.at(-1)?.dropped();
      vi.advanceTimersByTime(delay - 1);
      expect(FakeWebSocket.instances).toHaveLength(i + 1);
      vi.advanceTimersByTime(1);
      expect(FakeWebSocket.instances).toHaveLength(i + 2);
    }
  });

  test('stops after reconnectAttempts', () => {
    const client = new WebSocketClient('ws://test', {
      reconnectDelay: 10,
      reconnectAttempts: 2,
    });
    client.connect();
    FakeWebSocket.instances.at(-1)?.dropped();
    vi.advanceTimersByTime(10);
    FakeWebSocket.instances.at(-1)?.dropped();
    vi.advanceTimersByTime(20);
    FakeWebSocket.instances.at(-1)?.dropped();
    vi.advanceTimersByTime(60_000);
    // Initial connection + 2 retries, nothing more.
    expect(FakeWebSocket.instances).toHaveLength(3);
  });

  test('a successful open resets the backoff counter', () => {
    const client = new WebSocketClient('ws://test', {
      reconnectDelay: 1000,
      reconnectAttempts: 10,
    });
    client.connect();
    FakeWebSocket.instances.at(-1)?.dropped();
    vi.advanceTimersByTime(1000);
    FakeWebSocket.instances.at(-1)?.open();
    // After a successful open the next drop starts over at the base delay.
    FakeWebSocket.instances.at(-1)?.dropped();
    vi.advanceTimersByTime(1000);
    expect(FakeWebSocket.instances).toHaveLength(3);
  });
});

describe('on() unsubscribe', () => {
  test('returned closure removes exactly that handler', () => {
    const client = new WebSocketClient('ws://test');
    const seen: string[] = [];
    const offA = client.on('open', () => {
      seen.push('a');
    });
    client.on('open', () => {
      seen.push('b');
    });

    client.connect();
    FakeWebSocket.instances.at(-1)?.open();
    expect(seen).toEqual(['a', 'b']);

    offA();
    FakeWebSocket.instances.at(-1)?.dropped();
    client.connect();
    FakeWebSocket.instances.at(-1)?.open();
    expect(seen).toEqual(['a', 'b', 'b']);
  });
});

describe('disconnect/reconnect contract', () => {
  test('disconnect stops auto-reconnect; a later connect() restores it', () => {
    const client = new WebSocketClient('ws://test', { reconnectDelay: 10 });
    client.connect();
    FakeWebSocket.instances.at(-1)?.open();

    client.disconnect();
    vi.advanceTimersByTime(60_000);
    // No reconnect after an explicit disconnect.
    expect(FakeWebSocket.instances).toHaveLength(1);

    // Reconnecting the same instance re-enables auto-reconnect.
    client.connect();
    expect(FakeWebSocket.instances).toHaveLength(2);
    FakeWebSocket.instances.at(-1)?.dropped();
    vi.advanceTimersByTime(10);
    expect(FakeWebSocket.instances).toHaveLength(3);
  });
});
