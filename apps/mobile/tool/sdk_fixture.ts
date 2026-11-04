import { readFileSync } from 'node:fs';
// Import the reference checkout's source so stale workspace dist/ artifacts
// cannot silently select a different SDK API during these experiments.
const repo = process.env.OMOCHI_HUB_ROOT ?? new URL('../../../', import.meta.url).pathname;
const { App, schema } = await import(`${repo}/packages/sdk/typescript/src/index.ts`);
const { UnixSocketServerTransport } = await import(`${repo}/packages/sdk/typescript/src/node/index.ts`);

const app = new App({ temperature: 50, result: '' });
const image = process.env.OMOCHI_IMAGE
  ? app.resources.bytes(process.env.OMOCHI_IMAGE_FILE ? readFileSync(process.env.OMOCHI_IMAGE_FILE) : Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAEAAAAAgCAYAAACinX6EAAAAfUlEQVR4nO3RQQ2AQADEwDODHJSgEAk4wA044HeZI+ljn0027Tjv6/naduyf+zs/9AHNJ0AX0HwCdAHNJ0AX0PxY/eBsPgG6gOYToAtoPgG6gOYTsHqh2XwCdAHNJ0AX0HwCdAHNJ2D1QrP5BOgCmk+ALqD5BOgCmh/6gOZflSbVLXnqnYIAAAAASUVORK5CYII=', 'base64'), 'image/png')
  : null;
const run = app.action('run', schema.object({ temperature: schema.number() }), async (input, context) => {
  await context.update(state => {
    state.temperature = input.temperature;
    state.result = `Ran at temperature ${input.temperature}`;
  });
});
app.surface('main', (ui, state) => {
  const form = ui.local('form', { temperature: state.temperature });
  if (process.env.OMOCHI_MODEL_BYTES) ui.local('padding', { value: 'x'.repeat(Number(process.env.OMOCHI_MODEL_BYTES)) });
  ui.text('Flutter ↔ Omochi', { key: 'title', variant: 'h2' });
  ui.slider('temperature', 0, 100, { bind: form.field('temperature').bind() });
  ui.button('Run', run.call({ temperature: form.field('temperature').value() }), { variant: 'primary' });
  if (state.result) ui.text(state.result, { key: 'result' });
  if (image) ui.image(image, { key: 'resource-image', description: 'Image delivered over iroh', variant: 'icon' });
});
await app.start(new UnixSocketServerTransport({ serviceId: process.env.OMOCHI_SERVICE ?? 'flutter-example', name: 'Flutter integration' }));
