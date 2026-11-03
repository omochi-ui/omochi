import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { cpSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
export default defineConfig({ root: fileURLToPath(new URL('./hosted/', import.meta.url)), base: './',
  plugins: [react(), { name: 'copy-hub-transport', closeBundle() {
    cpSync(fileURLToPath(new URL('../hub-transport/generated/', import.meta.url)), fileURLToPath(new URL('./dist-hosted/iroh/', import.meta.url)), { recursive: true });
  } }],
  build: { outDir: fileURLToPath(new URL('./dist-hosted/', import.meta.url)), emptyOutDir: true },
});
