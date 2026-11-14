/** Component libraries (per project) and granted user libraries (ADR 0014). */
import { useEffect } from "preact/hooks";
import type { LibraryLayer } from "../api/project";
import {
  DEFAULT_PROJECT_LAYER,
  USER_LIBRARY_LAYER,
  layerStatusLabel,
  movedLayer,
  withLayer,
  withoutLayer,
} from "../model/library";
import {
  addLayerFromFolder,
  effectivePaths,
  grantLibrary,
  libraryGrants,
  librarySettings,
  loadLibraryGrants,
  loadLibrarySettings,
  revokeGrantOf,
  revokeLibrary,
  updateComponentPaths,
} from "../state/library-actions";
import { libraryDetails } from "../state/store";
import { Icon } from "../ui/Icon";
import { Modal } from "./Modal";
import { closeDialog, confirmAction } from "./dialogs";

function LayerRow({
  template,
  layer,
  index,
  count,
  paths,
}: {
  template: string;
  layer: LibraryLayer | undefined;
  index: number;
  count: number;
  paths: string[];
}) {
  const status = layerStatusLabel(layer);
  return (
    <li class="layer-row">
      <span class="layer-text">
        <code class="mono">{template}</code>
        {layer?.path && <small class="mono">{layer.path}</small>}
      </span>
      <span class={`pill ${status.tone === "muted" ? "" : status.tone}`}>
        {status.text}
      </span>
      <span class="layer-actions">
        <button
          class="icon-btn small"
          aria-label={`Move ${template} up`}
          title="Search earlier"
          disabled={index === 0}
          onClick={() =>
            void updateComponentPaths(movedLayer(paths, index, -1))
          }
        >
          <Icon name="chevronUp" size={15} />
        </button>
        <button
          class="icon-btn small"
          aria-label={`Move ${template} down`}
          title="Search later"
          disabled={index === count - 1}
          onClick={() => void updateComponentPaths(movedLayer(paths, index, 1))}
        >
          <Icon name="chevronDown" size={15} />
        </button>
        <button
          class="icon-btn small"
          aria-label={`Remove ${template}`}
          title="Remove from this project's libraries"
          onClick={() => void updateComponentPaths(withoutLayer(paths, index))}
        >
          <Icon name="trash" size={15} />
        </button>
      </span>
    </li>
  );
}

export function LibrariesDialog() {
  useEffect(() => {
    void loadLibrarySettings();
  }, []);
  const settings = librarySettings.value;
  const details = libraryDetails.value;
  const paths = effectivePaths(settings);
  const standard = details?.layers[details.layers.length - 1];
  const grant = settings?.grant;
  const usesUserLibrary = paths.some((entry) =>
    entry.startsWith(USER_LIBRARY_LAYER),
  );
  return (
    <Modal title="Component libraries" onClose={closeDialog} wide>
      <div class="modal-body">
        <p class="field-note">
          New parts come from these folders, searched from the top. When two
          folders hold a part with the same id, the higher one wins. Parts
          already on the board keep their own copy and never change.
        </p>
        {!settings?.project_file && (
          <p class="field-note warn">
            This project has not been saved yet, so folders inside its folder
            are not searched until you save it.
          </p>
        )}

        <h3 class="modal-subhead">This project</h3>
        <ol class="layer-list">
          {paths.map((template, index) => (
            <LayerRow
              key={`${index}:${template}`}
              template={template}
              layer={
                details?.layers.length === paths.length + 1
                  ? details.layers[index]
                  : undefined
              }
              index={index}
              count={paths.length}
              paths={paths}
            />
          ))}
          <li class="layer-row fixed">
            <span class="layer-text">
              <strong>Standard library</strong>
              <small class="mono">
                {standard?.version
                  ? `built in, v${standard.version}`
                  : (standard?.path ?? "built in")}
              </small>
            </span>
            <span class="pill">{layerStatusLabel(standard).text}</span>
            <span class="layer-actions" />
          </li>
        </ol>
        <div class="layer-buttons">
          <button class="btn ghost" onClick={() => void addLayerFromFolder()}>
            <Icon name="folder" size={15} /> Add folder…
          </button>
          {!usesUserLibrary && (
            <button
              class="btn ghost"
              onClick={() =>
                void updateComponentPaths(withLayer(paths, USER_LIBRARY_LAYER))
              }
            >
              <Icon name="plus" size={15} /> Add user library
            </button>
          )}
          {settings?.component_paths && (
            <button
              class="btn ghost"
              title={`Search only ${DEFAULT_PROJECT_LAYER}`}
              onClick={() => void updateComponentPaths(null)}
            >
              Reset to default
            </button>
          )}
        </div>

        <h3 class="modal-subhead">User library on this computer</h3>
        <p class="field-note">
          A folder of your own parts that projects can share. Each project needs
          its own grant, and grants stay on this computer: someone you send the
          project to sees only its own parts until they grant a library of
          theirs.
        </p>
        <div class="grant-row">
          {grant ? (
            <>
              <span class="layer-text">
                <strong>Granted</strong>
                <small class="mono">{grant.user_components}</small>
              </span>
              <button class="btn ghost" onClick={() => void grantLibrary()}>
                Change…
              </button>
              <button
                class="btn ghost danger-text"
                onClick={() => void revokeLibrary()}
              >
                Revoke
              </button>
            </>
          ) : (
            <>
              <span class="layer-text">
                <strong>Not granted</strong>
                <small>
                  {usesUserLibrary
                    ? "The ${USER_COMPONENTS} folders above are not searched."
                    : "Add the user library above to search it."}
                </small>
              </span>
              <button class="btn primary" onClick={() => void grantLibrary()}>
                Grant…
              </button>
            </>
          )}
        </div>
        <footer class="modal-foot">
          <button class="btn ghost" onClick={closeDialog}>
            Close
          </button>
        </footer>
      </div>
    </Modal>
  );
}

export function PreferencesDialog() {
  useEffect(() => {
    void loadLibraryGrants();
  }, []);
  const grants = libraryGrants.value;
  return (
    <Modal title="Preferences" onClose={closeDialog} wide>
      <div class="modal-body">
        <h3 class="modal-subhead">Granted user libraries</h3>
        <p class="field-note">
          Projects on this computer that may use a user library. Revoking a
          grant only stops new parts coming from it; parts already placed are
          unaffected.
        </p>
        {!grants.length && (
          <p class="empty-note">No project has been granted a user library.</p>
        )}
        <ul class="layer-list">
          {grants.map((entry) => (
            <li class="layer-row" key={entry.project_id}>
              <span class="layer-text">
                <strong>{entry.project_name || "Unnamed project"}</strong>
                <small class="mono">
                  {entry.user_components}
                  {!entry.exists && " (folder not found)"}
                </small>
                {entry.last_seen_path && (
                  <small class="mono">{entry.last_seen_path}</small>
                )}
              </span>
              <button
                class="btn ghost danger-text"
                onClick={async () => {
                  const confirmed = await confirmAction(
                    "Revoke grant",
                    `${entry.project_name || "This project"} will no longer take new parts from ${entry.user_components}.`,
                    "Revoke",
                    true,
                  );
                  if (confirmed) await revokeGrantOf(entry.project_id);
                }}
              >
                Revoke
              </button>
            </li>
          ))}
        </ul>
        <footer class="modal-foot">
          <button class="btn ghost" onClick={closeDialog}>
            Close
          </button>
        </footer>
      </div>
    </Modal>
  );
}
