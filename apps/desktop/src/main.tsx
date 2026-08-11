import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyTheme, readTheme } from "./lib/theme";
import "./styles.css";

// Before render, not in a component. A theme applied on mount means the first
// paint uses the default scheme and then jumps, which is the flash every app
// with a theme switch has to deal with once.
applyTheme(readTheme());

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
