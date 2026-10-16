import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    env: { TZ: 'UTC' },
    setupFiles: ['src/test-setup.ts'],
    // Node's own experimental localStorage only warns; tests use jsdom's or a stub.
    execArgv: ['--no-experimental-webstorage'],
  },
})
