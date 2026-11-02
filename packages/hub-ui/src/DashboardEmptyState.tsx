import { Button } from './Button.js';
import { Icon } from './Icon.js';

export function DashboardEmptyState({
  hasDashboard,
  board,
  disabled,
  onAdd,
}: {
  hasDashboard: boolean;
  board: boolean;
  disabled: boolean;
  onAdd: () => void;
}) {
  return (
    <div className="empty-state">
      <div className="empty-symbol">
        <Icon name="dashboard" />
      </div>
      <h2>{hasDashboard ? 'No views yet' : 'No dashboards yet'}</h2>
      <p>{board ? 'Exit board view to add views.' : 'Add views published by your apps.'}</p>
      {!board && (
        <Button className="primary with-icon" disabled={disabled} onClick={onAdd}>
          <Icon name="plus" /> {hasDashboard ? 'Add view' : 'Create dashboard'}
        </Button>
      )}
    </div>
  );
}
