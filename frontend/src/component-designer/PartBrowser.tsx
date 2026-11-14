/**
 * The Part designer's list of parts: the open project's libraries in search
 * order, plus a folder of parts the user picked (ADR 0014). Project and user
 * parts open for editing; standard parts are copied first.
 */
import { useEffect } from "preact/hooks";
import type { FolderPart, WritableLayerKind } from "../api/project";
import {
  browserSections,
  layerStatusLabel,
  layerTitle,
  type BrowserPart,
  type BrowserSection,
} from "../model/library";
import { openDialog } from "../shell/dialogs";
import { refreshLibrary } from "../state/actions";
import { libraryDetails } from "../state/store";
import { Icon } from "../ui/Icon";

export interface PartBrowserActions {
  open: (directory: string) => void;
  copy: (definitionId: string, target: WritableLayerKind) => void;
  newIn: (target: WritableLayerKind) => void;
  remove: (part: BrowserPart, sections: BrowserSection[]) => void;
  pickFolder: () => void;
  newInFolder: () => void;
}

function PartRow({
  part,
  editable,
  sections,
  actions,
  copyTargets,
}: {
  part: BrowserPart;
  editable: boolean;
  sections: BrowserSection[];
  actions: PartBrowserActions;
  copyTargets: WritableLayerKind[];
}) {
  const hiddenBy =
    part.hiddenBy === undefined ? undefined : sections[part.hiddenBy]?.layer;
  const detail = [
    part.id,
    part.pinCount === undefined ? undefined : `${part.pinCount} pins`,
    hiddenBy ? `overridden by the ${layerTitle(hiddenBy).toLowerCase()}` : "",
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <li class={`browser-part${hiddenBy ? " hidden" : ""}`}>
      {editable && part.directory ? (
        <button
          class="browser-open"
          title={part.directory}
          onClick={() => actions.open(part.directory!)}
        >
          <strong>{part.name}</strong>
          <small class="mono">{detail}</small>
        </button>
      ) : (
        <span class="browser-open static">
          <strong>{part.name}</strong>
          <small class="mono">{detail}</small>
        </span>
      )}
      {editable && part.directory && (
        <button
          class="icon-btn danger"
          title={`Move ${part.directory} to the trash`}
          aria-label={`Delete ${part.name}`}
          onClick={() => actions.remove(part, sections)}
        >
          <Icon name="trash" size={15} />
        </button>
      )}
      {!editable && !hiddenBy && (
        <span class="browser-copy">
          {copyTargets.map((target) => (
            <button
              key={target}
              class="btn ghost small"
              title={`Copy into the ${target === "project" ? "project's folder" : "user library"} to edit it there; the copy then replaces the standard part for this project`}
              onClick={() => actions.copy(part.id, target)}
            >
              Copy to {target === "project" ? "project" : "user library"}
            </button>
          ))}
        </span>
      )}
    </li>
  );
}

export function PartBrowser({
  actions,
  picked,
  current,
  onBack,
}: {
  actions: PartBrowserActions;
  picked: { path: string; parts: FolderPart[] } | null;
  current?: string;
  onBack: () => void;
}) {
  useEffect(() => {
    void refreshLibrary();
  }, []);
  const sections = browserSections(libraryDetails.value);
  const writable = (kind: WritableLayerKind) =>
    sections.find((section) => section.layer.kind === kind);
  const copyTargets = (["project", "user"] as const).filter((kind) => {
    const status = writable(kind)?.layer.status;
    return status === "loaded" || status === "missing";
  });
  return (
    <div class="part-browser">
      <header class="browser-head">
        <div>
          <h2>Parts</h2>
          <p>
            Pick a part to edit it. Parts in the project's folder and your user
            library are edited in place; copy a standard part to change it.
          </p>
        </div>
        <div class="button-row">
          {current && (
            <button class="btn ghost" onClick={onBack}>
              Back to {current}
            </button>
          )}
          <button class="btn ghost" onClick={() => openDialog("libraries")}>
            <Icon name="folder" size={15} /> Component libraries…
          </button>
        </div>
      </header>

      {picked && (
        <section class="browser-section">
          <header>
            <strong>Folder</strong>
            <small class="mono">{picked.path}</small>
          </header>
          <ul>
            {picked.parts.map((part) => (
              <li class="browser-part" key={part.directory}>
                <button
                  class="browser-open"
                  title={part.directory}
                  onClick={() => actions.open(part.directory)}
                >
                  <strong>{part.name}</strong>
                  <small class="mono">
                    {part.id} · {part.pin_count} pins
                  </small>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      {sections.map((section) => {
        const kind = section.layer.kind;
        const editable = kind !== "standard";
        const first =
          editable && writable(kind as WritableLayerKind) === section;
        const status = layerStatusLabel(section.layer);
        const canCreate =
          first &&
          (section.layer.status === "loaded" ||
            section.layer.status === "missing");
        return (
          <section class="browser-section" key={section.layerIndex}>
            <header>
              <strong>{layerTitle(section.layer)}</strong>
              <small class="mono">
                {section.layer.template ??
                  (section.layer.version
                    ? `built in, ${/^\d/.test(section.layer.version) ? "v" : ""}${section.layer.version}`
                    : section.layer.path)}
              </small>
              {section.layer.status !== "loaded" && (
                <span
                  class={`pill ${status.tone === "muted" ? "" : status.tone}`}
                >
                  {status.text}
                </span>
              )}
              {canCreate && (
                <button
                  class="btn ghost small"
                  onClick={() => actions.newIn(kind as WritableLayerKind)}
                >
                  <Icon name="plus" size={14} /> New part here
                </button>
              )}
            </header>
            {section.parts.length ? (
              <ul>
                {section.parts.map((part) => (
                  <PartRow
                    key={`${part.id}:${part.directory ?? ""}`}
                    part={part}
                    editable={editable}
                    sections={sections}
                    actions={actions}
                    copyTargets={copyTargets}
                  />
                ))}
              </ul>
            ) : (
              <p class="empty-note">No parts here yet.</p>
            )}
          </section>
        );
      })}

      <footer class="browser-foot">
        <button class="btn ghost" onClick={actions.pickFolder}>
          <Icon name="folder" size={15} /> Open part folder…
        </button>
        <button class="btn ghost" onClick={actions.newInFolder}>
          <Icon name="plus" size={15} /> New part in a folder…
        </button>
      </footer>
    </div>
  );
}
