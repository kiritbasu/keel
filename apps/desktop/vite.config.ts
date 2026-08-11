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
        // Server-sent events must not be compressed, or the proxy buffers a
        // whole gzip block before anything reaches the browser.
        //
        // Only `accept-encoding`. An earlier version of this also called
        // `res.flushHeaders()` in `proxyRes`, which looked like the obvious fix
        // and was the actual bug: it sends the response head *before* the proxy
        // has copied the upstream headers onto it, so the reply reached Chrome
        // with no `content-type` at all. `EventSource` needs
        // `text/event-stream` to accept a stream, so it sat in CONNECTING
        // forever — and curl did not care, which is why it looked fixed.
        configure: (proxy) => {
          proxy.on("proxyReq", (proxyReq, req) => {
            if (req.url?.startsWith("/api/events")) {
              proxyReq.setHeader("accept-encoding", "identity");
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
