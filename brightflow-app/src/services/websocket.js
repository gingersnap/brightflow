/**
 * WebSocket client with automatic reconnection
 */
export class WebSocketClient {
  constructor(url, options = {}) {
    this.url = url
    this.options = {
      reconnect: true,
      reconnectDelay: 1000,
      reconnectDelayMax: 30000,
      reconnectAttempts: 10,
      heartbeatInterval: 30000,
      ...options
    }

    this.ws = null
    this.reconnectCount = 0
    this.reconnectTimer = null
    this.heartbeatTimer = null
    this.messageQueue = []

    // Event handlers
    this.handlers = {
      open: [],
      close: [],
      error: [],
      message: []
    }
  }

  get isConnected() {
    return this.ws?.readyState === WebSocket.OPEN
  }

  get isConnecting() {
    return this.ws?.readyState === WebSocket.CONNECTING
  }

  connect() {
    if (this.ws?.readyState === WebSocket.OPEN || this.ws?.readyState === WebSocket.CONNECTING) {
      return
    }

    try {
      this.ws = new WebSocket(this.url)
      this.setupEventHandlers()
    } catch (error) {
      this.emit('error', error)
    }
  }

  disconnect() {
    this.options.reconnect = false
    this.clearTimers()

    if (this.ws) {
      this.ws.close(1000, 'Client disconnect')
      this.ws = null
    }
  }

  send(data) {
    const message = typeof data === 'string' ? data : JSON.stringify(data)

    if (this.isConnected) {
      this.ws.send(message)
    } else {
      // Queue message for when connection is established
      this.messageQueue.push(message)
    }
  }

  on(event, handler) {
    if (this.handlers[event]) {
      this.handlers[event].push(handler)
    }
    // Return unsubscribe function
    return () => {
      this.handlers[event] = this.handlers[event].filter(h => h !== handler)
    }
  }

  setupEventHandlers() {
    this.ws.onopen = () => {
      console.log('[WebSocket] Connection opened')
      this.reconnectCount = 0
      this.startHeartbeat()
      this.flushMessageQueue()
      this.emit('open')
    }

    this.ws.onclose = (event) => {
      console.log('[WebSocket] Connection closed', event.code, event.reason)
      this.clearTimers()
      this.emit('close', event)

      if (this.options.reconnect && !event.wasClean) {
        this.scheduleReconnect()
      }
    }

    this.ws.onerror = (event) => {
      console.error('[WebSocket] Error', event)
      this.emit('error', event)
    }

    this.ws.onmessage = (event) => {
      console.log('[WebSocket] Raw message:', event.data)
      try {
        const data = JSON.parse(event.data)

        // Handle pong silently
        if (data.type === 'pong') {
          return
        }

        this.emit('message', data)
      } catch {
        this.emit('message', event.data)
      }
    }
  }

  scheduleReconnect() {
    if (this.reconnectCount >= this.options.reconnectAttempts) {
      console.warn('WebSocket: Max reconnection attempts reached')
      return
    }

    const delay = Math.min(
      this.options.reconnectDelay * Math.pow(2, this.reconnectCount),
      this.options.reconnectDelayMax
    )

    this.reconnectTimer = setTimeout(() => {
      this.reconnectCount++
      this.connect()
    }, delay)
  }

  startHeartbeat() {
    this.heartbeatTimer = setInterval(() => {
      if (this.isConnected) {
        this.ws.send(JSON.stringify({ type: 'ping' }))
      }
    }, this.options.heartbeatInterval)
  }

  clearTimers() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer)
      this.heartbeatTimer = null
    }
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
  }

  flushMessageQueue() {
    while (this.messageQueue.length > 0) {
      const message = this.messageQueue.shift()
      this.ws.send(message)
    }
  }

  emit(event, data) {
    this.handlers[event].forEach(handler => {
      try {
        handler(data)
      } catch (error) {
        console.error(`WebSocket ${event} handler error:`, error)
      }
    })
  }
}

// Singleton instance
let client = null

export function getWebSocketClient() {
  if (!client) {
    const wsUrl = import.meta.env.VITE_WS_URL || 'ws://localhost:8080/api/ws'
    client = new WebSocketClient(wsUrl)
  }
  return client
}
