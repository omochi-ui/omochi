import assert from 'node:assert/strict';
import {
  MessageProcessor,
  GenericBinder,
  ComponentContext,
  ButtonApi,
  type A2uiClientAction,
  type A2uiMessage,
} from '@a2ui/web_core/v0_9';
import { test } from '@omochi-ui/core/testing';
import { resourceCatalog } from '../src/resources.js';
import { prepareRendererMessage } from '../src/messages.js';

test('shared SDK fixture object input remains reactive through the latest catalog validator', async (t) => {
  const fixture = await Bun.file(
    new URL('../../../spec/fixtures/basic.json', import.meta.url),
  ).json();
  const original = structuredClone(fixture.initialMessages);
  const actions: A2uiClientAction[] = [];
  const processor = new MessageProcessor([resourceCatalog], (action) => {
    actions.push(action);
  });
  t.after(() => processor.model.dispose());
  processor.processMessages(
    fixture.initialMessages.map((message: A2uiMessage) =>
      prepareRendererMessage(message),
    ),
  );
  assert.deepEqual(fixture.initialMessages, original);
  const surface = processor.model.getSurface('main')!;
  const binder = new GenericBinder<{ action: () => void }>(
    new ComponentContext(surface, 'action:run'),
    ButtonApi.schema,
  );
  t.after(() => binder.dispose());
  binder.snapshot.action();
  await Promise.resolve();
  assert.deepEqual(actions[0]?.context.input, { temperature: 50 });
  surface.dataModel.set('/$local/form/temperature', 75);
  binder.snapshot.action();
  await Promise.resolve();
  assert.deepEqual(actions[1]?.context.input, { temperature: 75 });
});

test('void SDK action input stays null through the latest DynamicValue schema', async (t) => {
  const fixture = await Bun.file(
    new URL('../../../spec/fixtures/basic.json', import.meta.url),
  ).json();
  fixture.initialMessages[1].updateComponents.components.find(
    (component: { id: string }) => component.id === 'action:run',
  ).action.event.context.input = null;
  const actions: A2uiClientAction[] = [];
  const processor = new MessageProcessor([resourceCatalog], (action) => {
    actions.push(action);
  });
  t.after(() => processor.model.dispose());
  processor.processMessages(
    fixture.initialMessages.map((message: A2uiMessage) =>
      prepareRendererMessage(message),
    ),
  );
  const binder = new GenericBinder<{ action: () => void }>(
    new ComponentContext(processor.model.getSurface('main')!, 'action:run'),
    ButtonApi.schema,
  );
  t.after(() => binder.dispose());
  binder.snapshot.action();
  await Promise.resolve();
  assert.equal(actions.length, 1);
  assert.equal(actions[0]!.context.input, null);
});
