import { render } from "preact";
import { App } from "./app/App";
import { startSync } from "./state/sync";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/board.css";
import "./styles/designer.css";
import "./styles/print.css";

async function boot(): Promise<void> {
  const native = "__TAURI_INTERNALS__" in window;
  // A plain browser tab (vite dev, no Tauri) gets the in-memory preview
  // backend so the UI can be developed and screenshotted without Rust.
  if (!native && import.meta.env.DEV) {
    const { installMockBackend } = await import("./dev/mock-backend");
    await installMockBackend();
  }
  render(<App />, document.getElementById("app")!);
  await startSync(!native);
}

void boot();
