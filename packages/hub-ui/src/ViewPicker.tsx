import { useEffect, useRef, useState } from 'react';
import { Button } from './Button.js';
import { Icon } from './Icon.js';
import { referenceId, type Dashboard } from './contracts.js';
import { restoreDialogFocus, type DialogProps } from './dialogs.js';
import { groupByNode, splitSurfaceId, type ServiceView } from './services.js';

export function ViewPicker({
  services,
  surfaceIds,
  draft,
  addSurface,
  onClose,
  returnTo,
}: DialogProps & {
  services: ServiceView[];
  surfaceIds: readonly string[];
  draft: Dashboard | null;
  addSurface: (service: ServiceView, id: string) => void;
}) {
  const [search, setSearch] = useState('');
  const pickerDialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    pickerDialog.current?.showModal();
  }, []);
  const closePicker = () => {
    pickerDialog.current?.close();
    onClose();
    restoreDialogFocus(returnTo);
  };
  const groups = groupByNode(services);
  const matches = services.flatMap((service) =>
    service.node && service.status === 'connected'
      ? surfaceIds.filter(
          (id) =>
            splitSurfaceId(id).channelId === service.id &&
            `${service.name} ${splitSurfaceId(id).surfaceId}`.toLowerCase().includes(search.toLowerCase()),
        )
      : [],
  );
  return (
    <dialog ref={pickerDialog} aria-labelledby="picker-title" className="picker" onCancel={closePicker}>
      <header>
        <div>
          <h2 id="picker-title">Add a view</h2>
        </div>
        <Button className="icon-button" aria-label="Close view picker" onClick={closePicker}>
          <Icon name="close" />
        </Button>
      </header>
      <p>Select a view from a connected app.</p>
      <label htmlFor="search-views">Search views</label>
      <input
        id="search-views"
        autoFocus
        className="search"
        aria-label="Search views"
        placeholder="App or view name"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <div className="view-options">
        {groups
          .filter((group) =>
            group.services.some((service) =>
              matches.some((id) => splitSurfaceId(id).channelId === service.id),
            ),
          )
          .map((group) => (
            <section key={group.node?.id ?? 'unknown'}>
              <h3>{group.node?.name ?? 'Unknown machine'}</h3>
              {group.services
                .filter((s) => s.node && s.status === 'connected')
                .map((service) => {
                  const ids = surfaceIds.filter(
                    (id) =>
                      splitSurfaceId(id).channelId === service.id &&
                      `${service.name} ${splitSurfaceId(id).surfaceId}`
                        .toLowerCase()
                        .includes(search.toLowerCase()),
                  );
                  return ids.length ? (
                    <div className="app-options" key={service.id}>
                      <h4>{service.name}</h4>
                      {ids.map((id) => {
                        const added = draft?.panels.some((p) => referenceId(p.reference) === id);
                        return (
                          <Button
                            static
                            key={id}
                            disabled={added}
                            onClick={() => {
                              addSurface(service, id);
                              restoreDialogFocus(returnTo);
                            }}
                          >
                            <span>
                              <Icon name="view" />
                              <strong>{splitSurfaceId(id).surfaceId}</strong>
                            </span>
                            <span>
                              {added ? (
                                'Added'
                              ) : (
                                <>
                                  <Icon name="plus" /> Add
                                </>
                              )}
                            </span>
                          </Button>
                        );
                      })}
                    </div>
                  ) : null;
                })}
            </section>
          ))}
        {!matches.length && (
          <div className="picker-empty" role="status">
            {search ? (
              <>
                <p>No views match “{search}”.</p>
                <Button onClick={() => setSearch('')}>Clear search</Button>
              </>
            ) : (
              <p>No views available. Start an app and check its connection to the hub.</p>
            )}
          </div>
        )}
      </div>
    </dialog>
  );
}
