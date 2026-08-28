import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Tauri expects a fixed, predictable dev URL.
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Never let Rust build output retrigger the frontend watcher.
      ignored: ['**/src-tauri/**'],
    },
  },

  // Keep the bundle debuggable in a Tauri debug build, lean in release.
  // Target is WebView2's Chromium, not the open web — no legacy browsers here.
  build: {
    target: 'chrome110',
    sourcemap: process.env.TAURI_ENV_DEBUG === 'true',
    minify: process.env.TAURI_ENV_DEBUG !== 'true',
  },
});
