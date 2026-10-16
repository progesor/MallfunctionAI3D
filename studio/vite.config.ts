import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: {
      // Vite must not watch the Rust/Tauri project. On Windows, Cargo holds
      // target/debug/deps/*.dll open while recompiling and chokidar's fs.watch
      // can throw EBUSY, terminating Tauri's beforeDevCommand.
      // Tauri's own Rust watcher continues to handle src-tauri changes.
      ignored: ['**/src-tauri', '**/src-tauri/**'],
    },
  },
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: { target: 'chrome105' },
});
