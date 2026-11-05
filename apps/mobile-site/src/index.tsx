import { Hono } from 'hono';
import { renderer } from './renderer';

const app = new Hono();

app.use(renderer);

app.get('/', (c) => {
  return c.render(
    <main>
      <img class="logo" src="/mark.svg" alt="" width="96" height="96" />
      <h1>Omochi Mobile</h1>
      <p>Connect to Omochi Dashboard from your smartphone</p>
      <a class="github" href="https://github.com/omochi-ui/omochi">
        GitHub <span aria-hidden="true">↗</span>
      </a>
    </main>,
  );
});

export default app;
