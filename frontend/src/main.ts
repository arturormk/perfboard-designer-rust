import "./styles.css";
import { getBootstrapInfo } from "./api/bootstrap";

const status = document.querySelector<HTMLParagraphElement>("#status");

if (status) {
  getBootstrapInfo()
    .then((info) => {
      status.textContent = `Rust engine ready · project schema v${info.schema_version}`;
      status.dataset.state = "ready";
    })
    .catch((error: unknown) => {
      status.textContent = `The desktop bridge is unavailable: ${String(error)}`;
      status.dataset.state = "error";
    });
}
