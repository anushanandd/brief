import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react()],
  optimizeDeps: {
    exclude: ['liveline'],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  // Only Tauri's build-environment variables; TAURI_SIGNING_* must never reach the bundle.
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    // The app loads its chunks from local disk, so size costs no download; routes still split.
    chunkSizeWarningLimit: 1024,
  },
})
