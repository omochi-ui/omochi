import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { resolve } from 'node:path';
import { test } from '@omochi-ui/core/testing';
import { ButtonApi, ComponentContext, GenericBinder, MessageProcessor } from '@a2ui/web_core/v0_9';
import { createActionMessage } from '../src/client.js';
import { resourceCatalog } from '../src/resources.js';
import { assertResolvedInput } from '../src/expressions.js';

// Python is an optional runtime for the renderer package. This runs explicitly
// in the Python CI matrix, using the installed wheel rather than source imports.
test('Python mount/input API round-trips actions while renderer drafts remain independent', {
  skip: !process.env.OMOCHI_PYTHON_TEST,
  timeout: 15000,
}, async t => {
  const root = resolve(import.meta.dirname, '../../..');
  const child = spawn(process.env.PYTHON ?? 'python3', ['-u', 'examples/python/app.py'], {
    cwd: root,
    env: {
      ...process.env,
      OMOCHI_SERVICE: '',
      PYTHONPATH: process.env.OMOCHI_PYTHON_INSTALLED_TEST ? '' : resolve(root, 'packages/sdk/python/src'),
    },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  const errors: string[] = [];
  const actions: unknown[] = [];
  const received: unknown[] = [];
  child.stderr.on('data', chunk => errors.push(String(chunk)));
  child.on('error', error => errors.push(String(error)));
  const processors = Array.from({ length: 2 }, () => {
    const processor = new MessageProcessor([resourceCatalog], action => {
      assertResolvedInput(action.context);
      actions.push(action.context.input);
      child.stdin.write(JSON.stringify(createActionMessage(action, processor.getClientDataModel())) + '\n');
    });
    processor.onSurfaceCreated(surface => surface.onError.subscribe(error => { errors.push(String(error.message)); }));
    return processor;
  });
  const lines = createInterface({ input: child.stdout });
  lines.on('line', line => {
    try {
      const message = JSON.parse(line);
      received.push(message);
      if (message.version === 'v0.9') {
        for (const processor of processors) processor.processMessages([structuredClone(message)]);
      }
    } catch (error) { errors.push(String(error)); }
  });
  const binders: GenericBinder<{ action: () => void }>[] = [];
  t.after(async () => {
    for (const binder of binders) binder.dispose();
    for (const processor of processors) {
      for (const surface of processor.model.surfacesMap.values()) surface.dispose();
    }
    lines.close();
    if (child.exitCode === null && child.signalCode === null) {
      const exited = new Promise<void>(resolve => child.once('exit', () => resolve()));
      child.stdin.end();
      const timer = setTimeout(() => child.kill('SIGKILL'), 2000);
      try { await exited; } finally { clearTimeout(timer); }
    }
  });
  const wait = async (condition: () => boolean) => {
    const deadline = Date.now() + 5000;
    while (!condition()) {
      if (errors.length || child.exitCode !== null || Date.now() > deadline) {
        throw new Error(`Python renderer did not reach expected state (exit=${child.exitCode}): ${errors.join('; ')}; actions=${JSON.stringify(actions)}; received=${JSON.stringify(received)}`);
      }
      await Bun.sleep(5);
    }
  };
  await wait(() => processors.every(p => p.model.surfacesMap.get('main')?.dataModel.get('/$local/form/temperature') === 50));
  const surfaces = processors.map(processor => processor.model.surfacesMap.get('main')!);
  for (const surface of surfaces) {
    binders.push(new GenericBinder<{ action: () => void }>(new ComponentContext(surface, 'action:run'), ButtonApi.schema));
  }
  surfaces[0]!.dataModel.set('/$local/form/temperature', 75);
  surfaces[1]!.dataModel.set('/$local/form/temperature', 20);
  // A schema error must not terminate the shared Python receive loop.
  child.stdin.write(JSON.stringify({ action: { name: 'run', context: { input: { temperature: 'invalid' } } } }) + '\n');
  binders[0]!.snapshot.action();
  const result = (index: number) => [...surfaces[index]!.componentsModel.entries]
    .find(([id]) => id === 'result')?.[1].properties.text;
  await wait(() => result(0) === 'Ran at temperature 75' && result(1) === 'Ran at temperature 75');
  assert.equal(surfaces[0]!.dataModel.get('/$local/form/temperature'), 75);
  assert.equal(surfaces[1]!.dataModel.get('/$local/form/temperature'), 20);
  binders[1]!.snapshot.action();
  await wait(() => result(0) === 'Ran at temperature 20' && result(1) === 'Ran at temperature 20');
  assert.deepEqual(actions, [{ temperature: 75 }, { temperature: 20 }]);
  assert.deepEqual(errors, []);
});
