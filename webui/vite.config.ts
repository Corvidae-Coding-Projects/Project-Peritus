import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  server: { host: '127.0.0.1', strictPort: true, proxy: { '/api': { target: 'http://127.0.0.1:4173', changeOrigin: true } } },
  preview: { host: '127.0.0.1', strictPort: true },
  build: {
    target: 'es2022',
    manifest: true,
    sourcemap: false,
  },
});
