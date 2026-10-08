import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  test: {
    include: ['tests/unit/**/*.test.ts','src/**/*.test.ts'],
    environment: 'node',
    maxWorkers: 2,
    clearMocks: true,
    restoreMocks: true,
  },
});
