/**
 * WebSocket lifecycle and message routing.
 *
 * One connection is shared by every feature that needs push updates (queries,
 * insights, curation, system metrics), with handlers registered by message type.
 * A socket per feature would multiply reconnect storms and server-side fan-out
 * for no benefit.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { createLogger } from '@/services/logger';
import { type WebSocketClient, createWebSocketClient } from '@/services/websocket';
import type { ConnectionStatus } from '@/types';

const log = createLogger('WS');

type MessageHandler = (message: Record<string, unknown>) => void;

export const useConnectionStore = defineStore('connection', () => {
  // State
  const status = ref<ConnectionStatus>('disconnected');
  const serverVersion = ref<string | null>(null);
  const lastError = ref<unknown>(null);
  const reconnectCount = ref(0);

  // Computed
  const isConnected = computed(() => status.value === 'connected');

  const statusColor = computed(() => {
    switch (status.value) {
      case 'connected': {
        return 'success';
      }
      case 'connecting': {
        return 'warning';
      }
      case 'error': {
        return 'error';
      }
      case 'disconnected': {
        return 'neutral';
      }
    }
  });

  // Client reference
  let client: WebSocketClient | null = null;

  // Message handlers
  const messageHandlers = new Map<string, MessageHandler[]>();

  function handleMessage(message: Record<string, unknown>): void {
    const type = String(message['type']);
    log.debug('Received:', type, message);

    switch (type) {
      case 'connected': {
        const sv = message['serverVersion'];
        serverVersion.value = typeof sv === 'string' ? sv : null;
        status.value = 'connected';
        break;
      }

      case 'queryResult':
      case 'error':
      case 'actionEvent':
      case 'actionBatch':
      case 'agentRun':
      case 'insightsComputed':
      case 'actionResync': {
        // Route to registered handlers
        const handlers = messageHandlers.get(type) ?? [];
        log.debug('Routing to', handlers.length, 'handlers');
        handlers.forEach((handler) => {
          handler(message);
        });
        break;
      }

      default: {
        log.debug('Unknown message type:', type);
      }
    }
  }

  // Actions
  function connect(): void {
    if (client && (client.isConnected || client.isConnecting)) {
      return;
    }

    status.value = 'connecting';

    // Create fresh client instance to avoid stale state
    client = createWebSocketClient();

    client.on('open', () => {
      status.value = 'connecting'; // Wait for 'connected' message
      reconnectCount.value = 0;
    });

    client.on('close', () => {
      status.value = 'disconnected';
    });

    client.on('error', (error: unknown) => {
      status.value = 'error';
      lastError.value = error;
      reconnectCount.value++;
    });

    client.on<Record<string, unknown>>('message', handleMessage);

    client.connect();
  }

  function disconnect(): void {
    if (client) {
      client.disconnect();
      client = null;
    }
    status.value = 'disconnected';
    messageHandlers.clear();
  }

  function send(message: unknown): void {
    if (client && client.isConnected) {
      log.debug('Sending:', message);
      client.send(message);
    }
  }

  function onMessage(type: string, handler: MessageHandler): () => void {
    if (!messageHandlers.has(type)) {
      messageHandlers.set(type, []);
    }
    const handlers = messageHandlers.get(type);
    if (handlers) {
      handlers.push(handler);
    }

    // Return unsubscribe function
    return () => {
      const currentHandlers = messageHandlers.get(type);
      if (currentHandlers) {
        const index = currentHandlers.indexOf(handler);
        if (index !== -1) {
          currentHandlers.splice(index, 1);
        }
      }
    };
  }

  return {
    connect,
    disconnect,
    isConnected,
    lastError,
    onMessage,
    reconnectCount,
    send,
    serverVersion,
    status,
    statusColor,
  };
});
