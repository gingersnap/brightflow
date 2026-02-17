import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { getWebSocketClient, type WebSocketClient } from '@/services/websocket';
import type { ConnectionStatus, WsMessage, ConnectedMessage } from '@/types';

type MessageHandler = (message: WsMessage) => void;

export const useConnectionStore = defineStore('connection', () => {
  // State
  const status = ref<ConnectionStatus>('disconnected');
  const serverVersion = ref<string | null>(null);
  const lastError = ref<unknown>(null);
  const reconnectCount = ref(0);

  // Computed
  const isConnected = computed(() => status.value === 'connected');
  const isConnecting = computed(() => status.value === 'connecting');
  const isDisconnected = computed(
    () => status.value === 'disconnected' || status.value === 'error',
  );

  const statusText = computed(() => {
    switch (status.value) {
      case 'connected':
        return 'Connected';
      case 'connecting':
        return 'Connecting...';
      case 'error':
        return 'Connection error';
      default:
        return 'Disconnected';
    }
  });

  const statusColor = computed(() => {
    switch (status.value) {
      case 'connected':
        return 'success';
      case 'connecting':
        return 'warning';
      case 'error':
        return 'error';
      default:
        return 'neutral';
    }
  });

  // Client reference
  let client: WebSocketClient | null = null;

  // Message handlers
  const messageHandlers = new Map<string, MessageHandler[]>();

  function handleMessage(message: WsMessage): void {
    const { type } = message;
    console.log('[WS] Received:', type, message);

    switch (type) {
      case 'connected': {
        const connectedMsg = message as ConnectedMessage;
        serverVersion.value = connectedMsg.serverVersion;
        status.value = 'connected';
        break;
      }

      case 'queryResult':
      case 'error':
      case 'metadata':
      case 'datasetList': {
        // Route to registered handlers
        const handlers = messageHandlers.get(type) ?? [];
        console.log('[WS] Routing to', handlers.length, 'handlers');
        handlers.forEach((handler) => handler(message));
        break;
      }

      default:
        console.log('[WS] Unknown message type:', type, message);
    }
  }

  // Actions
  function connect(): void {
    if (client && (client.isConnected || client.isConnecting)) {
      return;
    }

    status.value = 'connecting';
    client = getWebSocketClient();

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

    client.on<WsMessage>('message', handleMessage);

    client.connect();
  }

  function disconnect(): void {
    if (client) {
      client.disconnect();
      client = null;
    }
    status.value = 'disconnected';
  }

  function send(message: unknown): void {
    if (client && client.isConnected) {
      console.log('[WS] Sending:', message);
      client.send(message);
    } else {
      console.warn('Cannot send message: WebSocket not connected');
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
        if (index > -1) {
          currentHandlers.splice(index, 1);
        }
      }
    };
  }

  return {
    status,
    serverVersion,
    lastError,
    reconnectCount,
    isConnected,
    isConnecting,
    isDisconnected,
    statusText,
    statusColor,
    connect,
    disconnect,
    send,
    onMessage,
  };
});
