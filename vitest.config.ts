import { configDefaults, defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // A zone behind UTC, so a UTC date standing in for a local one fails in the evening.
    env: { TZ: 'America/Los_Angeles' },
    setupFiles: ['src/test-setup.ts'],
    // Agent worktrees live under .claude/ and carry their own copies of the tests.
    exclude: [...configDefaults.exclude, '.claude/**'],
    // Node's own experimental localStorage only warns; tests use jsdom's or a stub.
    execArgv: ['--no-experimental-webstorage'],
  },
})
