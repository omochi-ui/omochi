/** Handwritten declarations: ordinary builds never require generated WASM assets. */
export interface RawEndpoint {
  connect(address: string, alpn: string, operation: number): Promise<RawConnection>;
  cancel_connect(operation: number): void;
  close(): Promise<void>;
  frame_bytes(): number;
  free(): void;
}
export interface RawConnection {
  remote_id(): string;
  open(header: string, operation: number): Promise<RawStream>;
  cancel_open(operation: number): void;
  closed(): Promise<string>;
  close(): void;
  free(): void;
}
export interface RawStream {
  send(json: string): Promise<void>;
  receive(): Promise<string | null>;
  finish(): Promise<void>;
  cancel(): Promise<void>;
  free(): void;
}
export interface WasmModule {
  default(options: { module_or_path: URL }): Promise<unknown>;
  generate_identity(): string;
  identity_from_secret(secret: string): string;
  RawEndpoint: { create(secret: string, relays: string): Promise<RawEndpoint> };
  DownstreamState: { new(mode: string, permissions: string): RawDownstreamState };
  downstream_retryable(code: string): boolean;
}

export interface RawDownstreamState {
  accept(frame: string): string;
  acknowledge(sequence: string): void;
  close(): void;
  free(): void;
}
