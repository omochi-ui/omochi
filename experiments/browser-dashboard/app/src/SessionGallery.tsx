import { SurfaceView, type RendererSession } from '@omochi-ui/renderer';
import { useSyncExternalStore } from 'react';

/** Shared connection preview and test consumer of the production renderer. */
export function SessionGallery({
  controller,
}: {
  controller: RendererSession;
}) {
  const session = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
    controller.getSnapshot,
  );
  return (
    <main>
      <h1>Connected applications</h1>
      <p role="status">{session.status}</p>
      {session.error && <p role="alert">{session.error}</p>}
      {session.services.map((service) => (
        <p key={service.id}>
          {service.node.name}: {service.name} — {service.status}
          {service.detail ? `: ${service.detail}` : ''}
        </p>
      ))}
      {session.surfaceIds.map((surfaceId) => (
        <section key={surfaceId} data-surface={surfaceId}>
          <SurfaceView session={session} surfaceId={surfaceId} />
        </section>
      ))}
    </main>
  );
}
