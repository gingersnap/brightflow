/**
 * Live server metrics for the system panel, fed by WebSocket pushes.
 *
 * Rides the shared WebSocketClient (backoff + heartbeat) rather than owning a
 * socket; this store only decodes the two push shapes and caps the log buffer.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import {
  bindSocketStatus,
  createWebSocketClient,
  type WebSocketClient,
} from '@/services/websocket';
import type { ConnectionStatus } from '@/types';

export interface SystemMetrics {
  processRssBytes: number;
  processAnonBytes: number;
  systemUsedBytes: number;
  systemTotalBytes: number;
  cpuPercent: number;
  uptimeSecs: number;
}

export interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
}

const MAX_LOG_ENTRIES = 500;

export const useSystemStore = defineStore('system', () => {
  const metrics = ref<SystemMetrics | null>(null);
  const logs = ref<LogEntry[]>([]);
  const status = ref<ConnectionStatus>('disconnected');

  const isConnected = computed(() => status.value === 'connected');

  let client: WebSocketClient | null = null;

  function handleMessage(data: unknown): void {
    if (data == null || typeof data !== 'object') {
      return;
    }
    // oxlint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- JSON boundary
    const record = data as Record<string, unknown>;
    if (record['type'] === 'systemMetrics') {
      metrics.value = {
        cpuPercent: Number(record['cpuPercent']),
        processAnonBytes: Number(record['processAnonBytes']),
        processRssBytes: Number(record['processRssBytes']),
        systemTotalBytes: Number(record['systemTotalBytes']),
        systemUsedBytes: Number(record['systemUsedBytes']),
        uptimeSecs: Number(record['uptimeSecs']),
      };
    } else if (record['type'] === 'logEntry') {
      logs.value.push({
        level: String(record['level']),
        message: String(record['message']),
        target: String(record['target']),
        timestamp: String(record['timestamp']),
      });
      if (logs.value.length > MAX_LOG_ENTRIES) {
        logs.value = logs.value.slice(-MAX_LOG_ENTRIES);
      }
    }
  }

  function connect(): void {
    if (client == null) {
      client = createWebSocketClient('/api/system/ws');
      bindSocketStatus(client, status);
      client.on('message', handleMessage);
    }
    status.value = 'connecting';
    client.connect();
  }

  function disconnect(): void {
    client?.disconnect();
    status.value = 'disconnected';
  }

  function clearLogs(): void {
    logs.value = [];
  }

  return {
    clearLogs,
    connect,
    disconnect,
    isConnected,
    logs,
    metrics,
    status,
  };
});
