import { jsxRenderer } from 'hono/jsx-renderer';
import { Link, ViteClient } from 'vite-ssr-components/hono';

export const renderer = jsxRenderer(({ children }) => {
  return (
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>Omochi Mobile</title>
        <meta
          name="description"
          content="Connect to Omochi Dashboard from your smartphone"
        />
        <meta name="theme-color" content="#f5f6f2" />
        <link rel="canonical" href="https://mobile.omochi.fyi/" />
        <link rel="icon" href="/mark.svg" type="image/svg+xml" />
        <ViteClient />
        <Link href="/src/style.css" rel="stylesheet" />
      </head>
      <body>{children}</body>
    </html>
  );
});
