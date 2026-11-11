import { dismissToast, toasts } from "../state/toasts";
import { Icon, type IconName } from "../ui/Icon";

const ICONS: Record<string, IconName> = {
  info: "info",
  success: "check",
  error: "error",
  agent: "agent",
};

export function Toasts() {
  return (
    <div class="toasts" role="status" aria-live="polite">
      {toasts.value.map((toast) => (
        <div key={toast.id} class={`toast ${toast.tone}`}>
          <Icon name={ICONS[toast.tone]!} size={18} />
          <div class="toast-text">
            {toast.tone === "agent" && <span class="toast-kicker">Agent</span>}
            <strong>{toast.title}</strong>
            {toast.detail && <p>{toast.detail}</p>}
            {toast.actions && (
              <div class="toast-actions">
                {toast.actions.map((action) => (
                  <button
                    key={action.label}
                    class="link"
                    onClick={() => {
                      dismissToast(toast.id);
                      action.run();
                    }}
                  >
                    {action.label}
                  </button>
                ))}
              </div>
            )}
          </div>
          <button
            class="icon-btn small"
            aria-label="Dismiss"
            onClick={() => dismissToast(toast.id)}
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
