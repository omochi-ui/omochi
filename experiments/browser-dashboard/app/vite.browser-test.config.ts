import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { cpSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  root: fileURLToPath(new URL('./browser-test/', import.meta.url)),
  base: './',
  plugins: [
    react(),
    {
      name: 'copy-browser-transport',
      closeBundle() {
        cpSync(
          fileURLToPath(new URL('../transport/generated/', import.meta.url)),
          fileURLToPath(new URL('./dist-browser-test/iroh/', import.meta.url)),
          { recursive: true },
        );
      },
    },
  ],
  build: {
    outDir: fileURLToPath(new URL('./dist-browser-test/', import.meta.url)),
    emptyOutDir: true,
  },
});
