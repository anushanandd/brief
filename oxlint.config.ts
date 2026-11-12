import { defineConfig } from 'oxlint'

// Settings that change a result belong in FinanceSettings, which Rust saves; renderer storage
// holds only per-viewer conveniences, and every one goes through src/lib/preference.ts.
const storage = 'Use `preference` or `rendererStorage` from src/lib/preference.ts.'

export default defineConfig({
  options: {
    typeAware: true,
    typeCheck: true,
  },
  categories: {
    correctness: 'error',
    suspicious: 'warn',
  },
  rules: {
    // The macOS WebView answers window.confirm with Cancel; ask in the app instead.
    'no-alert': 'error',
    'no-restricted-globals': [
      'error',
      { name: 'localStorage', message: storage },
      { name: 'sessionStorage', message: storage },
    ],
    'no-restricted-properties': [
      'error',
      { object: 'window', property: 'localStorage', message: storage },
      { object: 'window', property: 'sessionStorage', message: storage },
      { object: 'globalThis', property: 'localStorage', message: storage },
      { object: 'globalThis', property: 'sessionStorage', message: storage },
    ],
  },
  overrides: [
    {
      // The module that owns renderer storage, and tests that stand it in or inspect it.
      files: [
        'src/lib/preference.ts',
        'src/test-setup.ts',
        'src/pages/routes.harness.tsx',
        '**/*.test.ts',
        '**/*.test.tsx',
      ],
      rules: { 'no-restricted-globals': 'off', 'no-restricted-properties': 'off' },
    },
  ],
  ignorePatterns: ['dist/**', 'src-tauri/target/**'],
})
