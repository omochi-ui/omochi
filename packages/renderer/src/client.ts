import { BrowserImages } from './images.js'
import { resourceCatalog } from './resources.js'
import { type ReactComponentImplementation } from '@a2ui/react/v0_9'
import {
  MessageProcessor,
  type A2uiClientAction,
  type A2uiMessage,
} from '@a2ui/web_core/v0_9'

export type Processor = MessageProcessor<ReactComponentImplementation>

export interface A2uiWebSocketClientCallbacks {
  onProcessor(processor: Processor | null): void
  onStatus(status: string): void
  onError(message: string | null): void
  onImages?(images: BrowserImages | null): void
}

export function isA2uiMessage(value: unknown): value is A2uiMessage {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  const object = value as Record<string, unknown>
  if (object.version !== 'v0.9') return false
  const keys = ['createSurface', 'updateComponents', 'updateDataModel', 'deleteSurface']
  return keys.filter((key) => key in object).length === 1
}

export function createActionMessage(
  action: A2uiClientAction,
  clientDataModel: unknown,
) {
  return {
    version: 'v0.9' as const,
    action,
    metadata: {
      a2uiClientDataModel: clientDataModel ?? {
        version: 'v0.9',
        surfaces: {},
      },
    },
  }
}

export class A2uiWebSocketClient {
  private readonly url: string
  private readonly callbacks: A2uiWebSocketClientCallbacks
  private socket: WebSocket | null = null
  private processor: Processor | null = null
  private images: BrowserImages | null = null
  private sequence = 0
  private resyncs = 0

  private retry: ReturnType<typeof setTimeout> | null = null
  private stopped = false

  constructor(url: string, callbacks: A2uiWebSocketClientCallbacks) {
    this.url = url
    this.callbacks = callbacks
  }

  connect(): void {
    if (this.socket || this.stopped) return
    const url = new URL(this.url, globalThis.location?.href ?? 'http://localhost')
    url.searchParams.set('images', '1')
    const socket = new WebSocket(url.href)
    this.socket = socket
    socket.onopen = () => {
      this.sequence = 0
      this.processor = new MessageProcessor<ReactComponentImplementation>(
        [resourceCatalog],
        (action: A2uiClientAction) => this.sendAction(action),
      )
      this.callbacks.onProcessor(this.processor)
      this.callbacks.onStatus('Connected')
      this.callbacks.onError(null)
    }
    socket.onmessage = (event) => {
      // One connection can carry several services, so a message the processor rejects
      // must not take the others down with it. Report it and keep the stream open.
      try {
        const message: unknown = JSON.parse(String(event.data))
        if (this.images) {
          if (!message || typeof message !== 'object' || Array.isArray(message)) throw new Error('Invalid image transport message')
          const envelope = message as Record<string, unknown>
          if (envelope.omochi !== 'images/1') throw new Error('Invalid image transport message')
          if (envelope.kind === 'image') { this.images.response(envelope); return }
          if (envelope.kind !== 'update' || envelope.sequence !== this.sequence + 1) throw new Error('Image transport sequence gap')
          this.sequence++
          try {
            if (envelope.message !== null) {
              if (!isA2uiMessage(envelope.message)) throw new Error('expected one A2UI v0.9 server message')
              const value = envelope.message as unknown as Record<string, Record<string, unknown>>
              const kind = ['createSurface', 'updateComponents', 'updateDataModel', 'deleteSurface'].find(key => key in value)!
              const surface = value[kind]!.surfaceId
              if (typeof surface !== 'string' || (envelope.handle !== null && typeof envelope.handle !== 'string')) throw new Error('Invalid Surface scope')
              this.images.update(surface, kind === 'deleteSurface' ? null : envelope.handle as string | null)
              this.processor?.processMessages([envelope.message])
            }
          } finally { socket.send(JSON.stringify({ omochi: 'images/1', kind: 'ack', sequence: this.sequence })) }
          return
        }
        // In legacy mode, service JSON is always data even when it has extra control-like fields.
        if (!isA2uiMessage(message) && message && typeof message === 'object' && 'omochi' in message && 'kind' in message && message.kind === 'hello') {
          if (message.omochi === 'resources/1') return
          if (message.omochi === 'images/1') {
            this.images = new BrowserImages(this.url, value => {
              if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(value))
            })
            this.callbacks.onImages?.(this.images)
            return
          }
        }
        if (!isA2uiMessage(message)) throw new Error('expected one A2UI v0.9 server message')
        if (!this.processor) throw new Error('received an A2UI message before the connection opened')
        this.processor.processMessages([message])
      } catch (error) {
        this.callbacks.onError(error instanceof Error ? error.message : String(error))
      }
    }
    socket.onerror = () => this.callbacks.onError('WebSocket connection failed')
    socket.onclose = (event) => {
      const scoped = this.images !== null
      this.images?.close()
      this.images = null
      this.callbacks.onImages?.(null)
      this.socket = null
      this.processor = null
      this.callbacks.onProcessor(null)
      this.callbacks.onStatus(`Disconnected${event.reason ? `: ${event.reason}` : ''}`)
      const capacityFailure = scoped && event.code === 1009
      const repeatedGap = scoped && event.code === 1013 && ++this.resyncs > 1
      if (capacityFailure || repeatedGap) this.callbacks.onError('Rendering capacity exceeded. Reload to retry.')
      else if (!this.stopped) this.retry = setTimeout(() => this.connect(), 1000)
    }
  }

  close(): void {
    this.stopped = true
    if (this.retry) clearTimeout(this.retry)
    this.images?.close()
    this.images = null
    this.callbacks.onImages?.(null)
    if (!this.socket) return
    this.socket.onopen = null
    this.socket.onmessage = null
    this.socket.onerror = null
    this.socket.onclose = null
    this.socket.close()
    this.socket = null
    this.processor = null
  }

  private sendAction(action: A2uiClientAction): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN || !this.processor) {
      this.callbacks.onError('cannot send an action while WebSocket is disconnected')
      return
    }
    const message = createActionMessage(action, this.processor.getClientDataModel())
    this.socket.send(JSON.stringify(message))
  }
}
