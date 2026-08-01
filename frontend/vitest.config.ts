import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
    css: true,
    coverage: {
      provider: 'v8',
      // lcovonly: Sonar が読むのは lcov.info のみ。'lcov' だと HTML レポートまで生成され
      // coverage/ 配下の生成物が ESLint に拾われてしまう
      reporter: ['text', 'lcovonly'],
      reportsDirectory: './coverage',
    },
  },
});
