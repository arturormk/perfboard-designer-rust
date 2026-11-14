import { useSignal } from "@preact/signals";
import type { ComponentDefinition } from "../api/project";
import { refreshLibrary, select, focusOn } from "../state/actions";
import {
  design,
  hover,
  library,
  libraryDetails,
  placing,
  selection,
  tool,
  changeKinds,
  showChanges,
} from "../state/store";
import { groupPlacedParts } from "../model/naming";
import { libraryProblems, partProvenance } from "../model/library";
import { Icon } from "../ui/Icon";
import { openDialog } from "../shell/dialogs";

export function PartThumb({
  definition,
  size = 44,
}: {
  definition: ComponentDefinition;
  size?: number;
}) {
  const pitch = 2.54;
  const { body } = definition;
  const xs = [
    body.x / pitch,
    (body.x + body.width) / pitch,
    ...definition.pins.map((pin) => pin.x),
  ];
  const ys = [
    body.y / pitch,
    (body.y + body.height) / pitch,
    ...definition.pins.map((pin) => pin.y),
  ];
  const minX = Math.min(...xs) - 0.6;
  const minY = Math.min(...ys) - 0.6;
  const width = Math.max(...xs) - minX + 0.6;
  const height = Math.max(...ys) - minY + 0.6;
  const side = Math.max(width, height);
  const bridge = definition.kind === "bridge";
  return (
    <svg
      class="part-thumb"
      width={size}
      height={size}
      viewBox={`${minX - (side - width) / 2} ${minY - (side - height) / 2} ${side} ${side}`}
      aria-hidden="true"
    >
      {bridge ? (
        <line
          class="thumb-bridge"
          x1={definition.pins[0]?.x ?? 0}
          y1={definition.pins[0]?.y ?? 0}
          x2={definition.pins[1]?.x ?? 1}
          y2={definition.pins[1]?.y ?? 0}
        />
      ) : (
        <rect
          class="thumb-body"
          x={body.x / pitch}
          y={body.y / pitch}
          width={body.width / pitch}
          height={body.height / pitch}
          rx={0.3}
        />
      )}
      {definition.pins.map((pin, index) => (
        <circle
          key={pin.id}
          class={index === 0 ? "thumb-pin first" : "thumb-pin"}
          cx={pin.x}
          cy={pin.y}
          r={bridge ? 0.28 : 0.26}
        />
      ))}
    </svg>
  );
}

export function PartsPanel() {
  const query = useSignal("");
  const openGroups = useSignal<Record<string, boolean>>({});
  const current = design.value;
  const needle = query.value.trim().toLowerCase();
  const definitions = library.value.filter(
    (definition) =>
      !needle ||
      definition.name.toLowerCase().includes(needle) ||
      definition.description.toLowerCase().includes(needle) ||
      definition.id.toLowerCase().includes(needle),
  );
  const details = libraryDetails.value;
  const problems = libraryProblems(details);
  const groups = current ? groupPlacedParts(current) : [];
  const selectedId =
    selection.value?.kind === "component" ? selection.value.id : undefined;

  return (
    <div class="panel-body">
      <section class="panel-section">
        <header class="section-head">
          <h3>Library</h3>
          <span class="section-actions">
            <button
              class="icon-btn small"
              title="Component libraries: where new parts come from"
              aria-label="Component libraries"
              onClick={() => openDialog("libraries")}
            >
              <Icon name="folder" size={15} />
            </button>
            <button
              class="icon-btn small"
              title="Rescan component folders"
              aria-label="Rescan component library"
              onClick={() => void refreshLibrary(true)}
            >
              <Icon name="refresh" size={15} />
            </button>
          </span>
        </header>
        <div class="search">
          <Icon name="search" size={15} />
          <input
            type="search"
            placeholder="Search parts"
            value={query.value}
            onInput={(event) =>
              (query.value = (event.target as HTMLInputElement).value)
            }
          />
        </div>
        <ul class="library-list">
          {definitions.map((definition) => {
            const bridge = definition.kind === "bridge";
            const provenance = partProvenance(details, definition.id);
            const active =
              placing.value?.id === definition.id ||
              (bridge && tool.value === "bridge");
            return (
              <li key={definition.id}>
                <button
                  class={`library-item${active ? " on" : ""}`}
                  title={[definition.description, provenance?.detail]
                    .filter(Boolean)
                    .join("\n\n")}
                  onClick={() => {
                    if (bridge) {
                      placing.value = null;
                      tool.value = "bridge";
                    } else {
                      tool.value = "select";
                      placing.value =
                        placing.value?.id === definition.id ? null : definition;
                    }
                  }}
                >
                  <PartThumb definition={definition} />
                  <span class="library-text">
                    <span class="library-name">
                      <strong>{definition.name}</strong>
                      {provenance?.badge && (
                        <span
                          class={`badge${provenance.overrides ? " warn" : ""}`}
                        >
                          {provenance.badge}
                        </span>
                      )}
                    </span>
                    <small>
                      {bridge
                        ? "Use the bridge tool (B)"
                        : `${definition.pins.length} pins · ${(definition.body.width / 2.54).toFixed(0)}×${(definition.body.height / 2.54).toFixed(0)} holes`}
                    </small>
                  </span>
                </button>
              </li>
            );
          })}
          {!definitions.length && (
            <li class="empty-note">
              {library.value.length
                ? "No parts match."
                : "No component library found."}
            </li>
          )}
        </ul>
        {problems.length > 0 && (
          <button
            class="library-problems"
            onClick={() => openDialog("libraries")}
            title={problems
              .map((problem) =>
                problem.path
                  ? `${problem.message}\n  ${problem.path}`
                  : problem.message,
              )
              .join("\n")}
          >
            <span
              class={`pill ${problems.some((problem) => problem.severity === "error") ? "error" : "warn"}`}
            >
              {problems.length === 1
                ? "1 library problem"
                : `${problems.length} library problems`}
            </span>
            <span>{problems[0]?.message}</span>
          </button>
        )}
      </section>

      <section class="panel-section grow">
        <header class="section-head">
          <h3>On the board</h3>
          <span class="count">
            {Object.keys(current?.components ?? {}).length}
          </span>
        </header>
        {!groups.length && (
          <p class="empty-note">
            Pick a part above, then click a hole to place it.
          </p>
        )}
        {groups.map((group) => {
          const open = openGroups.value[group.definitionId] ?? !group.bridge;
          return (
            <div class="part-group" key={group.definitionId}>
              <button
                class="group-head"
                aria-expanded={open}
                onClick={() =>
                  (openGroups.value = {
                    ...openGroups.value,
                    [group.definitionId]: !open,
                  })
                }
              >
                <Icon name={open ? "chevronDown" : "chevron"} size={14} />
                <span>{group.definition?.name ?? group.definitionId}</span>
                <span class="count">{group.instances.length}</span>
              </button>
              {open && (
                <ul class="part-list">
                  {group.instances.map((instance) => {
                    const change = showChanges.value
                      ? changeKinds.value.get(instance.id)
                      : undefined;
                    return (
                      <li key={instance.id}>
                        <button
                          class={`part-row${selectedId === instance.id ? " on" : ""}`}
                          onClick={() => {
                            select({ kind: "component", id: instance.id });
                            focusOn({ kind: "component", id: instance.id });
                          }}
                          onMouseEnter={() =>
                            (hover.value = {
                              kind: "component",
                              id: instance.id,
                            })
                          }
                          onMouseLeave={() => (hover.value = null)}
                        >
                          <span class="part-name">{instance.name}</span>
                          {change && (
                            <span class={`diff-dot ${change}`} title={change} />
                          )}
                          <span class="part-pos">
                            {instance.position.x},{instance.position.y}
                          </span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              )}
            </div>
          );
        })}
      </section>
    </div>
  );
}
