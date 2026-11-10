import type { DownstreamState } from '@omochi-ui/hub-transport';

/** Renderer integration fixture. Protocol rejection is tested against the Rust core. */
export function remoteStateFixture(): DownstreamState {
  const leases = new Map<string, string>();
  return {
    accept(frame) {
      if (frame.kind === 'snapshotEnd')
        return {
          sequence: String(frame.sequence),
          surface: null,
          lease: null,
          invalidatedLeases: [],
          snapshotEnd: true,
        };
      const message = frame.message as Record<string, Record<string, unknown>>;
      const kind = [
        'createSurface',
        'updateComponents',
        'updateDataModel',
        'deleteSurface',
      ].find((key) => key in message)!;
      const surface = message[kind]!.surfaceId as string;
      const old = leases.get(surface);
      const lease = kind === 'deleteSurface' ? null : (frame.lease as string);
      if (lease) leases.set(surface, lease);
      else leases.delete(surface);
      return {
        sequence: String(frame.sequence),
        surface,
        lease,
        invalidatedLeases:
          old && (old !== lease || kind === 'createSurface') ? [old] : [],
        snapshotEnd: false,
      };
    },
    acknowledge() {},
    close() {
      leases.clear();
    },
  };
}
