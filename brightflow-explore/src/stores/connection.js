import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { getWebSocketClient } from '@/services/websocket'
import { useDatasetStore } from './dataset'

export const useConnectionStore = defineStore('connection', () => {
  // State
  const status = ref('disconnected') // 'connected' | 'connecting' | 'disconnected' | 'error'
  const serverVersion = ref(null)
  const lastError = ref(null)
  const reconnectCount = ref(0)

  // Computed
  const isConnected = computed(() => status.value === 'connected')
  const isConnecting = computed(() => status.value === 'connecting')
  const isDisconnected = computed(() => status.value === 'disconnected' || status.value === 'error')

  const statusText = computed(() => {
    switch (status.value) {
      case 'connected': return 'Connected'
      case 'connecting': return 'Connecting...'
      case 'error': return 'Connection error'
      default: return 'Disconnected'
    }
  })

  const statusColor = computed(() => {
    switch (status.value) {
      case 'connected': return 'success'
      case 'connecting': return 'warning'
      case 'error': return 'error'
      default: return 'neutral'
    }
  })

  // Client reference
  let client = null

  // Message handlers
  const messageHandlers = new Map()

  function handleMessage(message) {
    const { type } = message
    console.log('[WS] Received:', type, message)

    switch (type) {
      case 'connected':
        serverVersion.value = message.serverVersion
        status.value = 'connected'
        // Fetch initial metadata
        const datasetStore = useDatasetStore()
        datasetStore.fetchMetadata()
        break

      case 'queryResult':
      case 'error':
      case 'metadata':
      case 'datasetList':
        // Route to registered handlers
        const handlers = messageHandlers.get(type) || []
        console.log('[WS] Routing to', handlers.length, 'handlers')
        handlers.forEach(handler => handler(message))
        break

      default:
        console.log('[WS] Unknown message type:', type, message)
    }
  }

  // Actions
  function connect() {
    if (client && (client.isConnected || client.isConnecting)) {
      return
    }

    status.value = 'connecting'
    client = getWebSocketClient()

    client.on('open', () => {
      status.value = 'connecting' // Wait for 'connected' message
      reconnectCount.value = 0
    })

    client.on('close', () => {
      status.value = 'disconnected'
    })

    client.on('error', (error) => {
      status.value = 'error'
      lastError.value = error
      reconnectCount.value++
    })

    client.on('message', handleMessage)

    client.connect()
  }

  function disconnect() {
    if (client) {
      client.disconnect()
      client = null
    }
    status.value = 'disconnected'
  }

  function send(message) {
    if (client && client.isConnected) {
      console.log('[WS] Sending:', message)
      client.send(message)
    } else {
      console.warn('Cannot send message: WebSocket not connected')
    }
  }

  function onMessage(type, handler) {
    if (!messageHandlers.has(type)) {
      messageHandlers.set(type, [])
    }
    messageHandlers.get(type).push(handler)

    // Return unsubscribe function
    return () => {
      const handlers = messageHandlers.get(type)
      const index = handlers.indexOf(handler)
      if (index > -1) {
        handlers.splice(index, 1)
      }
    }
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
    onMessage
  }
})
