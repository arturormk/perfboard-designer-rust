import { render } from "preact";
import { App } from "./app/App";
import { getBootstrapInfo } from "./api/bootstrap";
import { appVersion } from "./state/store";
import { startSync } from "./state/sync";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/board.css";
import "./styles/designer.css";
import "./styles/print.css";
import "./styles/assembly.css";

async function boot(): Promise<void> {
  const native = "__TAURI_INTERNALS__" in window;
  // A plain browser tab (vite dev, no Tauri) gets the in-memory preview
  // backend so the UI can be developed and screenshotted without Rust.
  if (!native && import.meta.env.DEV) {
    const { installMockBackend } = await import("./dev/mock-backend");
    await installMockBackend();
  }
  render(<App />, document.getElementById("app")!);
  getBootstrapInfo().then(
    (info) => (appVersion.value = info.version),
    () => undefined, // The version is informational; never block startup.
  );
  await startSync(!native);
}

void boot();
