import type { BrowserImages } from './images.js';
import { createLocalHubSession } from './local-source.js';
import {
  createA2uiSession,
  type Processor,
  type RendererSession,
} from './session.js';
export { createActionMessage, isA2uiMessage } from './messages.js';
export type { Processor } from './session.js';

export interface A2uiWebSocketClientCallbacks {
  onProcessor(processor: Processor | null): void;
  onStatus(status: string): void;
  onError(message: string | null): void;
  onImages?(images: BrowserImages | null): void;
}

/** Compatibility controller for consumers of the original WebSocket-only API. */
export class A2uiWebSocketClient {
  private session: RendererSession | null = null;
  private unsubscribe: (() => void) | null = null;
  private stopped = false;
  constructor(
    private readonly url: string,
    private readonly callbacks: A2uiWebSocketClientCallbacks,
  ) {}
  connect(): void {
    if (this.session || this.stopped) return;
    const session = createA2uiSession(createLocalHubSession(this.url));
    this.session = session;
    let previousProcessor: Processor | null | undefined;
    let previousImages: BrowserImages | null | undefined;
    let previousStatus: string | undefined;
    let previousError: string | null | undefined;
    const changed = () => {
      const snapshot = session.getSnapshot();
      const processor =
        snapshot.status === 'Connected' ? snapshot.processor : null;
      if (processor !== previousProcessor) {
        previousProcessor = processor;
        this.callbacks.onProcessor(processor);
      }
      if (snapshot.images !== previousImages) {
        previousImages = snapshot.images;
        this.callbacks.onImages?.(snapshot.images);
      }
      if (snapshot.status !== previousStatus) {
        previousStatus = snapshot.status;
        this.callbacks.onStatus(snapshot.status);
      }
      if (snapshot.error !== previousError) {
        previousError = snapshot.error;
        this.callbacks.onError(snapshot.error);
      }
    };
    this.unsubscribe = session.subscribe(changed);
    changed();
  }
  close(): void {
    this.stopped = true;
    const session = this.session;
    this.session = null;
    this.unsubscribe?.();
    this.unsubscribe = null;
    this.callbacks.onImages?.(null);
    this.callbacks.onProcessor(null);
    if (session) void session.close().catch(() => {});
  }
}
