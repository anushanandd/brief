import { defineConfig } from 'oxlint'

export default defineConfig({
  options: {
    typeAware: true,
    typeCheck: true,
  },
  categories: {
    correctness: 'error',
    suspicious: 'warn',
  },
  ignorePatterns: ['dist/**', 'src-tauri/target/**'],
})
