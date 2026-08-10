/// <reference types="vitest/config" />
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
  test: {
    // jsdom rather than a real browser: everything under test here is routing,
    // ranking and keyboard handling, none of which needs a compositor. The
    // parts that do need one — layout, the light theme — are not things a unit
    // test would catch anyway.
    environment: "jsdom",
    setupFiles: ["src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    restoreMocks: true,
  },
});
