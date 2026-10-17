import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { isTauri } from "./lib/ipc";

async function boot(): Promise<void> {
  // Browser dev/test mode: install the mock backend BEFORE React mounts
  // (never in Tauri production; the dynamic import is DEV-gated so the
  // mock never ships in a bundle). This is the "second mock" — the
  // packaged dev build talks to the Rust mock-core instead.
  if (!isTauri() && import.meta.env.DEV) {
    const { installMock } = await import("./mock/commands");
    installMock();
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void boot();
