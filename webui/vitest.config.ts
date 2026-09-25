import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/unit/**/*.test.ts','src/**/*.test.ts'],
    environment: 'node',
    maxWorkers: 2,
    clearMocks: true,
    restoreMocks: true,
  },
});
