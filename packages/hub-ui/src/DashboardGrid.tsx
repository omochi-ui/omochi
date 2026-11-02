import { useEffect, useState } from 'react';
import { SurfaceView, type A2uiSession } from '@omochi-ui/renderer';
import { Button } from './Button.js';
import { Icon } from './Icon.js';
import { referenceId, type Panel } from './contracts.js';
import { resizePanel, type ResizeEdge } from './layout.js';
import type { ServiceView } from './services.js';
import type { DashboardEditor } from './useDashboardEditor.js';
import { usePanelGesture } from './usePanelGesture.js';

export function DashboardGrid({
  panels,
  editor,
  session,
  services,
  busy,
  narrow,
  onDetails,
}: {
  panels: Panel[];
  editor: DashboardEditor;
  session: A2uiSession;
  services: ServiceView[];
  busy: boolean;
  narrow: boolean;
  onDetails: (panel: Panel) => void;
}) {
  const { draft, validated, changePanel, reorder } = editor;
  const editing = draft !== null;
  const connected = session.status === 'Connected';
  const [adjust, setAdjust] = useState<string | null>(null);
  useEffect(() => {
    setAdjust(null);
  }, [editing]);
  const { grid, preview, beginGesture, moveGesture, endGesture, cancelGesture, cancelOnEscape } =
    usePanelGesture({
      panels,
      editing,
      busy,
      narrow,
      onBegin: editor.beginLayout,
      onCommit: editor.commitLayout,
    });
  return (
    <div onKeyDown={cancelOnEscape} ref={grid} className={`panel-grid ${editing ? 'editing' : ''}`}>
      {panels.map((panel) => {
        const id = referenceId(panel.reference);
        const service = services.find(
          (s) => s.id === `${panel.reference.nodeId}~${panel.reference.serviceId}`,
        );
        const present = connected && service?.status === 'connected' && session.surfaceIds.includes(id);
        const rectangle = preview?.find((p) => p.id === panel.id) ?? panel;
        return (
          <article
            className="panel"
            key={panel.id}
            data-panel-id={panel.id}
            style={{
              gridColumn: `${rectangle.x + 1} / span ${rectangle.w}`,
              gridRow: `${rectangle.y + 1} / span ${rectangle.h}`,
            }}
          >
            <header className="panel-header">
              {editing && !narrow && (
                <Button
                  static
                  aria-label={`Move ${panel.title}`}
                  className="drag-handle"
                  disabled={busy}
                  aria-describedby="layout-help"
                  onKeyDown={(event) => {
                    const delta = (
                      {
                        ArrowLeft: [-1, 0],
                        ArrowRight: [1, 0],
                        ArrowUp: [0, -1],
                        ArrowDown: [0, 1],
                      } as Record<string, number[]>
                    )[event.key];
                    if (delta) {
                      event.preventDefault();
                      changePanel({
                        ...panel,
                        x: panel.x + delta[0]!,
                        y: Math.min(100000, panel.y + delta[1]!),
                      });
                    }
                  }}
                  onPointerDown={(e) => beginGesture(e, panel, 'move')}
                  onPointerMove={moveGesture}
                  onPointerUp={endGesture}
                  onPointerCancel={cancelGesture}
                  onLostPointerCapture={cancelGesture}
                >
                  <Icon name="grip" />
                </Button>
              )}
              <div className="panel-heading">
                {editing ? (
                  <input
                    aria-invalid={validated && !panel.title.trim()}
                    aria-describedby={validated && !panel.title.trim() ? `error-${panel.id}` : undefined}
                    aria-label={`View name ${panel.title}`}
                    maxLength={200}
                    disabled={busy}
                    value={panel.title}
                    onChange={(e) => changePanel({ ...panel, title: e.target.value })}
                  />
                ) : (
                  <h2>{panel.title}</h2>
                )}
                {editing && validated && !panel.title.trim() && (
                  <p className="field-error" id={`error-${panel.id}`}>
                    Enter a view name.
                  </p>
                )}
                <small
                  title={`${service?.node?.name ?? panel.reference.nodeId} / ${service?.name ?? panel.reference.serviceId}`}
                >
                  {service?.node?.name ?? panel.reference.nodeId} /{' '}
                  {service?.name ?? panel.reference.serviceId}
                </small>
              </div>
              <Button
                className="details-button"
                aria-label={`View details for ${panel.title}`}
                onClick={() => {
                  onDetails(panel);
                }}
              >
                Details
              </Button>
              {editing ? (
                <Button
                  className="icon-button"
                  aria-label={`Remove ${panel.title}`}
                  disabled={busy}
                  onClick={() => {
                    setAdjust(null);
                    editor.removePanel(panel);
                    requestAnimationFrame(() =>
                      (
                        document.querySelector<HTMLButtonElement>('.panel .details-button') ??
                        document.querySelector<HTMLButtonElement>('#add-view')
                      )?.focus(),
                    );
                  }}
                >
                  <Icon name="close" />
                </Button>
              ) : (
                <span className="view-status">{present ? '✓ Live' : '○ Unavailable'}</span>
              )}
            </header>
            <div className="panel-content">
              {present ? (
                <SurfaceView session={session} surfaceId={id} />
              ) : (
                <div className="unavailable">
                  <Icon name="unavailable" />
                  <strong>View unavailable</strong>
                  <p>
                    Start the source app and check its connection to restore this view.
                    {service?.detail && <small>{service.detail}</small>}
                  </p>
                </div>
              )}
            </div>
            {editing && (
              <footer className="panel-editor">
                {narrow ? (
                  <>
                    <Button
                      disabled={busy || panels[0]?.id === panel.id}
                      onClick={() => reorder(panel.id, -1)}
                    >
                      Move up
                    </Button>
                    <Button
                      disabled={busy || panels[panels.length - 1]?.id === panel.id}
                      onClick={() => reorder(panel.id, 1)}
                    >
                      Move down
                    </Button>
                  </>
                ) : (
                  <>
                    <Button
                      aria-expanded={adjust === panel.id}
                      aria-controls={`adjust-${panel.id}`}
                      disabled={busy}
                      onClick={() => setAdjust(adjust === panel.id ? null : panel.id)}
                    >
                      Adjust layout
                    </Button>
                    {adjust === panel.id && (
                      <div
                        id={`adjust-${panel.id}`}
                        className="adjust-controls"
                        role="group"
                        aria-label={`Adjust ${panel.title}`}
                      >
                        {(
                          [
                            ['Move left', { ...panel, x: panel.x - 1 }, panel.x === 0],
                            ['Move right', { ...panel, x: panel.x + 1 }, panel.x + panel.w === 12],
                            ['Move up', { ...panel, y: panel.y - 1 }, panel.y === 0],
                            ['Move down', { ...panel, y: panel.y + 1 }, panel.y >= 100000],
                            ['Narrower', resizePanel(panel, 'e', -1, 0), panel.w === 1],
                            ['Wider', resizePanel(panel, 'e', 1, 0), panel.x + panel.w === 12],
                            ['Shorter', resizePanel(panel, 's', 0, -1), panel.h === 1],
                            ['Taller', resizePanel(panel, 's', 0, 1), panel.h === 1000],
                          ] as [string, Panel, boolean][]
                        ).map(([label, next, boundary]) => (
                          <Button key={label} disabled={busy || boundary} onClick={() => changePanel(next)}>
                            {label}
                          </Button>
                        ))}
                      </div>
                    )}
                  </>
                )}
              </footer>
            )}
            {editing &&
              !narrow &&
              (['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as ResizeEdge[]).map((edge) => (
                <Button
                  static
                  key={edge}
                  className={`resize-edge resize-${edge}`}
                  aria-label={`Resize ${panel.title} ${edge}`}
                  tabIndex={-1}
                  disabled={busy}
                  onPointerDown={(e) => beginGesture(e, panel, edge)}
                  onPointerMove={moveGesture}
                  onPointerUp={endGesture}
                  onPointerCancel={cancelGesture}
                  onLostPointerCapture={cancelGesture}
                />
              ))}
          </article>
        );
      })}
    </div>
  );
}
