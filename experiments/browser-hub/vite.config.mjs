import { defineConfig } from '../../packages/hub-ui/node_modules/vite/dist/node/index.js';
import react from '../../packages/hub-ui/node_modules/@vitejs/plugin-react/dist/index.js';
import { fileURLToPath } from 'node:url';
const local = path => fileURLToPath(new URL(path, import.meta.url));
export default defineConfig({
  root: local('./web/'),
  plugins: [react()],
  resolve: { alias: {
    react: local('../../packages/hub-ui/node_modules/react'),
    'react-dom': local('../../packages/hub-ui/node_modules/react-dom'),
  } },
  build: { outDir: local('./dist/'), emptyOutDir: true, target: 'esnext' },
});
