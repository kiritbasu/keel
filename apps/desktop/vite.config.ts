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
        // Server-sent events need the proxy to stop buffering and to stop
        // compressing. Without this, `/api/events` connects and then sits in
        // CONNECTING forever: the daemon sends its headers immediately, the
        // proxy holds them waiting for a response body that will not arrive
        // until the first event, and the browser's EventSource never fires
        // `open`. Live refresh was silently dead in `npm run dev` — the app
        // looked fine because navigating refetches anyway.
        //
        // `selfHandleResponse: false` is the default and is stated here only
        // because setting it true is the usual accidental cause of the same
        // symptom.
        configure: (proxy) => {
          proxy.on("proxyReq", (proxyReq, req) => {
            if (req.url?.startsWith("/api/events")) {
              proxyReq.setHeader("accept-encoding", "identity");
            }
          });
          proxy.on("proxyRes", (proxyRes, req, res) => {
            if (req.url?.startsWith("/api/events")) {
              res.setHeader("cache-control", "no-cache, no-transform");
              res.setHeader("x-accel-buffering", "no");
              res.flushHeaders?.();
            }
          });
        },
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
