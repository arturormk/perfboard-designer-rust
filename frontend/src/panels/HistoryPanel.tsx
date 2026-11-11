import type { ChangeSummary } from "../api/project";
import {
  compareWith,
  discardChanges,
  redo,
  restoreCommit,
  undo,
} from "../state/actions";
import {
  canCommit,
  changeCount,
  compareCommitId,
  historyHighlight,
  showChanges,
  splitCompare,
  view,
} from "../state/store";
import { relativeTime } from "../model/naming";
import { Icon } from "../ui/Icon";
import { confirmAction, openDialog } from "../shell/dialogs";

function touched(entry: ChangeSummary): Set<string> {
  return new Set(
    [
      ...entry.diff.components,
      ...entry.diff.wires,
      ...entry.diff.rails,
      ...entry.diff.nets,
    ].map((item) => item.object_id),
  );
}

function OriginBadge({ origin }: { origin: ChangeSummary["origin"] }) {
  const agent = origin === "mcp";
  return (
    <span
      class={`origin ${agent ? "agent" : "user"}`}
      title={agent ? "Made by an AI agent over MCP" : "Made in this window"}
    >
      <Icon name={agent ? "agent" : "user"} size={13} />
    </span>
  );
}

function Entry({ entry, undone }: { entry: ChangeSummary; undone?: boolean }) {
  const details = [
    ...entry.diff.components,
    ...entry.diff.nets,
    ...entry.diff.wires,
    ...entry.diff.rails,
    ...entry.diff.constraints,
  ];
  return (
    <li
      class={`history-entry${undone ? " undone" : ""}${entry.origin === "mcp" ? " agent" : ""}`}
      onMouseEnter={() => (historyHighlight.value = touched(entry))}
      onMouseLeave={() => (historyHighlight.value = new Set())}
    >
      <OriginBadge origin={entry.origin} />
      <div class="entry-text">
        <strong>{entry.summary}</strong>
        {entry.rationale && <p class="rationale">“{entry.rationale}”</p>}
        {details.length > 1 && (
          <small>
            {details
              .slice(0, 3)
              .map((item) => item.summary)
              .join(" · ")}
            {details.length > 3 ? ` · +${details.length - 3} more` : ""}
          </small>
        )}
      </div>
    </li>
  );
}

export function HistoryPanel() {
  const current = view.value;
  if (!current) return null;
  const head = current.commits.find(
    (commit) => commit.id === current.head_commit,
  );
  const undoEntries = [...current.undo].reverse();
  const redoEntries = [...current.redo];
  const commits = [...current.commits].reverse();
  const agentCount = current.undo.filter(
    (entry) => entry.origin === "mcp",
  ).length;
  const comparing = compareCommitId.value;

  return (
    <div class="panel-body">
      <section class="panel-section">
        <div class="working-card">
          <div class="working-head">
            <Icon name="commit" />
            <div>
              <strong>Working copy</strong>
              <span>
                {head
                  ? `${changeCount.value} change${changeCount.value === 1 ? "" : "s"} since “${head.name}”`
                  : "Nothing committed yet"}
                {agentCount ? ` · ${agentCount} by the agent` : ""}
              </span>
            </div>
          </div>
          <div class="row wrap">
            <button
              class="btn primary small"
              disabled={!canCommit.value}
              onClick={() => openDialog("commit")}
            >
              Commit…
            </button>
            <button
              class="btn small"
              disabled={!current.undo.length}
              onClick={() => void undo()}
            >
              <Icon name="undo" size={14} /> Undo
            </button>
            <button
              class="btn small"
              disabled={!current.redo.length}
              onClick={() => void redo()}
            >
              <Icon name="redo" size={14} /> Redo
            </button>
            <button
              class="btn small ghost danger-text"
              disabled={!current.undo.length}
              onClick={async () => {
                if (
                  await confirmAction(
                    "Discard uncommitted changes?",
                    "The working copy returns to the last commit and the undo history is cleared. This cannot be undone.",
                    "Discard changes",
                    true,
                  )
                )
                  void discardChanges();
              }}
            >
              Discard
            </button>
          </div>
        </div>
      </section>

      <section class="panel-section">
        <header class="section-head">
          <h3>Uncommitted steps</h3>
          <span class="count">{current.undo.length}</span>
        </header>
        {!undoEntries.length && !redoEntries.length && (
          <p class="empty-note">Edits by you and by the agent appear here.</p>
        )}
        <ul class="history-list">
          {redoEntries.map((entry) => (
            <Entry key={entry.operation_id} entry={entry} undone />
          ))}
          {undoEntries.map((entry) => (
            <Entry key={entry.operation_id} entry={entry} />
          ))}
        </ul>
        {(current.undo.length > 0 || current.redo.length > 0) && (
          <button class="link small" onClick={() => openDialog("history")}>
            Manage stored history…
          </button>
        )}
      </section>

      <section class="panel-section grow">
        <header class="section-head">
          <h3>Commits</h3>
          <span class="count">{commits.length}</span>
        </header>
        {!commits.length && (
          <p class="empty-note">
            Commit to save a named checkpoint you can compare with or return to.
          </p>
        )}
        <ol class="commit-list">
          {commits.map((commit) => {
            const isHead = commit.id === current.head_commit;
            const isCompared = (comparing ?? current.head_commit) === commit.id;
            return (
              <li
                key={commit.id}
                class={`commit${isHead ? " head" : ""}${isCompared && (showChanges.value || splitCompare.value) ? " compared" : ""}`}
              >
                <span class="commit-dot" />
                <div class="commit-body">
                  <div class="commit-title">
                    <strong>{commit.name}</strong>
                    {isHead && <span class="badge">current base</span>}
                    {commit.origins.includes("mcp") && (
                      <span
                        class="badge agent"
                        title="Includes changes made by an AI agent"
                      >
                        <Icon name="agent" size={11} /> agent
                      </span>
                    )}
                  </div>
                  <small>
                    {relativeTime(commit.timestamp_ms)} · {commit.change_count}{" "}
                    step{commit.change_count === 1 ? "" : "s"}
                  </small>
                  {commit.description && <p>{commit.description}</p>}
                  {commit.rationales.length > 0 && (
                    <details>
                      <summary>
                        Agent explanations ({commit.rationales.length})
                      </summary>
                      <ul>
                        {commit.rationales.map((text, index) => (
                          <li key={index}>{text}</li>
                        ))}
                      </ul>
                    </details>
                  )}
                  <div class="row">
                    <button
                      class="btn small ghost"
                      onClick={async () => {
                        await compareWith(isHead ? undefined : commit.id);
                        showChanges.value = true;
                        splitCompare.value = true;
                      }}
                    >
                      <Icon name="compare" size={14} /> Compare
                    </button>
                    {!isHead && (
                      <button
                        class="btn small ghost"
                        onClick={async () => {
                          if (
                            await confirmAction(
                              `Restore “${commit.name}”?`,
                              "The working copy is replaced by this commit's design. It is one undoable step and no commit is lost.",
                              "Restore",
                            )
                          )
                            void restoreCommit(commit.id);
                        }}
                      >
                        Restore
                      </button>
                    )}
                  </div>
                </div>
              </li>
            );
          })}
        </ol>
      </section>
    </div>
  );
}
