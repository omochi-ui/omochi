import { UI } from '@omochi-ui/sdk';
import {
  NESTEDKEYCATALOG,
  type NestedKeyCatalogComponentMap,
} from './catalog.js';

const ui = new UI<NestedKeyCatalogComponentMap>('main', NESTEDKEYCATALOG);
ui.component('Shortcut', { key: 'shortcut', details: { key: 'Enter' } });
function invalidCalls() {
  // @ts-expect-error The node key is still required separately from the nested key.
  ui.component('Shortcut', { details: { key: 'Enter' } });
  // @ts-expect-error Nested key retains its schema type.
  ui.component('Shortcut', { key: 'wrong', details: { key: 1 } });
  // @ts-expect-error Nested key remains required.
  ui.component('Shortcut', { key: 'missing', details: {} });
}
void invalidCalls;
console.log(JSON.stringify(ui.build().root.slots![0]!.nodes));
