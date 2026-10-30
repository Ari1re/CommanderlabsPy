import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Used by both the Tauri app and the web edition. In dev, /api is proxied to
// a running `commander-lab-web --no-browser`.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || false,
    proxy: { '/api': 'http://127.0.0.1:7878' },
  },
  build: {
    target: ['es2021', 'chrome100', 'safari15'],
    outDir: 'dist',
    emptyOutDir: true,
  },
});
