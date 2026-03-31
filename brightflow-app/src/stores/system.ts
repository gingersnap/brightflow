import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

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

type SystemStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

const MAX_LOG_ENTRIES = 500;

export const useSystemStore = defineStore('system', () => {
  const metrics = ref<SystemMetrics | null>(null);
  const logs = ref<LogEntry[]>([]);
  const status = ref<SystemStatus>('disconnected');

  const isConnected = computed(() => status.value === 'connected');

  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let reconnectCount = 0;

  function getWsUrl(): string {
    const base =
      import.meta.env.VITE_WS_URL ||
      `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/api/ws`;
    // Replace /api/ws with /api/system/ws
    return base.replace(/\/api\/ws$/, '/api/system/ws');
  }

  function connect(): void {
    if (ws?.readyState === WebSocket.OPEN || ws?.readyState === WebSocket.CONNECTING) {
      return;
    }

    status.value = 'connecting';
    reconnectCount = 0;

    try {
      ws = new WebSocket(getWsUrl());

      ws.onopen = () => {
        status.value = 'connected';
        reconnectCount = 0;
      };

      ws.onmessage = (event: MessageEvent) => {
        try {
          const data = JSON.parse(event.data as string) as Record<string, unknown>;

          if (data['type'] === 'systemMetrics') {
            metrics.value = {
              cpuPercent: data['cpuPercent'] as number,
              processAnonBytes: data['processAnonBytes'] as number,
              processRssBytes: data['processRssBytes'] as number,
              systemTotalBytes: data['systemTotalBytes'] as number,
              systemUsedBytes: data['systemUsedBytes'] as number,
              uptimeSecs: data['uptimeSecs'] as number,
            };
          } else if (data['type'] === 'logEntry') {
            const entry: LogEntry = {
              level: data['level'] as string,
              message: data['message'] as string,
              target: data['target'] as string,
              timestamp: data['timestamp'] as string,
            };
            logs.value.push(entry);
            // Cap the array
            if (logs.value.length > MAX_LOG_ENTRIES) {
              logs.value = logs.value.slice(-MAX_LOG_ENTRIES);
            }
          }
        } catch {
          // Ignore parse errors
        }
      };

      ws.onclose = () => {
        status.value = 'disconnected';
        scheduleReconnect();
      };

      ws.onerror = () => {
        status.value = 'error';
      };
    } catch {
      status.value = 'error';
    }
  }

  function disconnect(): void {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    reconnectCount = 10; // Prevent further reconnects
    if (ws) {
      ws.close(1000, 'Client disconnect');
      ws = null;
    }
    status.value = 'disconnected';
  }

  function scheduleReconnect(): void {
    if (reconnectCount >= 10) {
      return;
    }
    const delay = Math.min(1000 * 2 ** reconnectCount, 30_000);
    reconnectCount++;
    reconnectTimer = setTimeout(() => {
      connect();
    }, delay);
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
