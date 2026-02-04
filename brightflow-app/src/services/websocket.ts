/**
 * WebSocket client with automatic reconnection
 */

type WsEventType = 'open' | 'close' | 'error' | 'message';
type WsHandler<T = unknown> = (data: T) => void;

interface WebSocketOptions {
  reconnect: boolean;
  reconnectDelay: number;
  reconnectDelayMax: number;
  reconnectAttempts: number;
  heartbeatInterval: number;
}

export class WebSocketClient {
  private url: string;
  private options: WebSocketOptions;
  private ws: WebSocket | null = null;
  private reconnectCount = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private messageQueue: string[] = [];
  private handlers: Record<WsEventType, WsHandler[]> = {
    open: [],
    close: [],
    error: [],
    message: [],
  };

  constructor(url: string, options: Partial<WebSocketOptions> = {}) {
    this.url = url;
    this.options = {
      reconnect: true,
      reconnectDelay: 1000,
      reconnectDelayMax: 30000,
      reconnectAttempts: 10,
      heartbeatInterval: 30000,
      ...options,
    };
  }

  get isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  get isConnecting(): boolean {
    return this.ws?.readyState === WebSocket.CONNECTING;
  }

  connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN || this.ws?.readyState === WebSocket.CONNECTING) {
      return;
    }

    try {
      this.ws = new WebSocket(this.url);
      this.setupEventHandlers();
    } catch (error) {
      this.emit('error', error);
    }
  }

  disconnect(): void {
    this.options.reconnect = false;
    this.clearTimers();

    if (this.ws) {
      this.ws.close(1000, 'Client disconnect');
      this.ws = null;
    }
  }

  send(data: unknown): void {
    const message = typeof data === 'string' ? data : JSON.stringify(data);

    if (this.isConnected && this.ws) {
      this.ws.send(message);
    } else {
      // Queue message for when connection is established
      this.messageQueue.push(message);
    }
  }

  on<T>(event: WsEventType, handler: WsHandler<T>): () => void {
    this.handlers[event].push(handler as WsHandler);
    // Return unsubscribe function
    return () => {
      this.handlers[event] = this.handlers[event].filter((h) => h !== handler);
    };
  }

  private setupEventHandlers(): void {
    if (!this.ws) return;

    this.ws.onopen = (): void => {
      console.log('[WebSocket] Connection opened');
      this.reconnectCount = 0;
      this.startHeartbeat();
      this.flushMessageQueue();
      this.emit('open', undefined);
    };

    this.ws.onclose = (event: CloseEvent): void => {
      console.log('[WebSocket] Connection closed', event.code, event.reason);
      this.clearTimers();
      this.emit('close', event);

      if (this.options.reconnect && !event.wasClean) {
        this.scheduleReconnect();
      }
    };

    this.ws.onerror = (event: Event): void => {
      console.error('[WebSocket] Error', event);
      this.emit('error', event);
    };

    this.ws.onmessage = (event: MessageEvent): void => {
      console.log('[WebSocket] Raw message:', event.data);
      try {
        const data = JSON.parse(event.data as string) as Record<string, unknown>;

        // Handle pong silently
        if (data['type'] === 'pong') {
          return;
        }

        this.emit('message', data);
      } catch {
        this.emit('message', event.data);
      }
    };
  }

  private scheduleReconnect(): void {
    if (this.reconnectCount >= this.options.reconnectAttempts) {
      console.warn('WebSocket: Max reconnection attempts reached');
      return;
    }

    const delay = Math.min(
      this.options.reconnectDelay * Math.pow(2, this.reconnectCount),
      this.options.reconnectDelayMax,
    );

    this.reconnectTimer = setTimeout(() => {
      this.reconnectCount++;
      this.connect();
    }, delay);
  }

  private startHeartbeat(): void {
    this.heartbeatTimer = setInterval(() => {
      if (this.isConnected && this.ws) {
        this.ws.send(JSON.stringify({ type: 'ping' }));
      }
    }, this.options.heartbeatInterval);
  }

  private clearTimers(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private flushMessageQueue(): void {
    while (this.messageQueue.length > 0) {
      const message = this.messageQueue.shift();
      if (message !== undefined && this.ws) {
        this.ws.send(message);
      }
    }
  }

  private emit(event: WsEventType, data: unknown): void {
    this.handlers[event].forEach((handler) => {
      try {
        handler(data);
      } catch (error) {
        console.error(`WebSocket ${event} handler error:`, error);
      }
    });
  }
}

// Singleton instance
let client: WebSocketClient | null = null;

export function getWebSocketClient(): WebSocketClient {
  if (!client) {
    const wsUrl = import.meta.env.VITE_WS_URL || 'ws://localhost:8080/api/ws';
    client = new WebSocketClient(wsUrl);
  }
  return client;
}
