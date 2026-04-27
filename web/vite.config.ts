import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// API lives at /api/* on the same origin in production (paint.coreon.build).
// In dev, proxy /api -> local wasmtime serve on :18080.
export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:18080',
        changeOrigin: false,
      },
      '/healthz': 'http://127.0.0.1:18080',
    },
  },
  preview: {
    host: '127.0.0.1',
    port: 5174,
  },
});
