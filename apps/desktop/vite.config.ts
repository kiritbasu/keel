import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The dev server proxies /api to the daemon so the browser and the Tauri
// webview hit the same relative paths. That is what lets the desktop build and
// a future remote web build be the same bundle with a different base URL
// (SPEC §10).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:7654",
        changeOrigin: false,
      },
    },
  },
  build: { outDir: "dist", emptyOutDir: true },
});
