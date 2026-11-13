import { leftTab } from "../state/store";
import { openDialog as openFile } from "../state/actions";
import { Icon } from "../ui/Icon";
import { openDialog } from "./dialogs";
import { otherRecentProjects, RecentProjectList } from "./RecentProjects";

/** Shown over an empty board: the three ways a design usually starts. */
export function Welcome() {
  const recent = otherRecentProjects().slice(0, 5);
  return (
    <div class="welcome" role="note">
      <h2>An empty board</h2>
      <p>
        Place a few parts, connect them, then route the wires you will solder.
      </p>
      <ol class="welcome-steps">
        <li>
          <Icon name="chip" size={18} />
          <div>
            <strong>Place parts</strong>
            <span>Pick from the library on the left and click a hole.</span>
          </div>
          <button class="btn small" onClick={() => (leftTab.value = "parts")}>
            Library
          </button>
        </li>
        <li>
          <Icon name="agent" size={18} />
          <div>
            <strong>Or ask your agent</strong>
            <span>
              Over MCP it can place parts and propose nets. You approve
              electrical changes.
            </span>
          </div>
        </li>
        <li>
          <Icon name="board" size={18} />
          <div>
            <strong>Different board?</strong>
            <span>Start a new size or open an existing project.</span>
          </div>
          <span class="button-row">
            <button class="btn small" onClick={() => openDialog("new-board")}>
              New…
            </button>
            <button class="btn small ghost" onClick={() => void openFile()}>
              Open…
            </button>
          </span>
        </li>
      </ol>
      {recent.length > 0 && (
        <section class="welcome-recent" aria-label="Recent projects">
          <h3>Recent projects</h3>
          <RecentProjectList projects={recent} />
        </section>
      )}
    </div>
  );
}
