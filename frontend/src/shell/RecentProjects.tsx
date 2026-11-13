import type { RecentProject } from "../api/project";
import { forgetRecent, openRecent } from "../state/actions";
import { filePath, recentProjects } from "../state/store";
import { Icon } from "../ui/Icon";

/** The folder that holds `path`, for telling same-named projects apart. */
export function parentFolder(path: string): string {
  const parts = path.split(/[\\/]/);
  parts.pop();
  return parts.pop() ?? "";
}

/** Recent projects other than the one already open. */
export function otherRecentProjects(): RecentProject[] {
  return recentProjects.value.filter(
    (project) => project.path !== filePath.value,
  );
}

/**
 * One row per recent project: open it, or remove it from the list. Missing
 * files stay listed, marked, until opened or removed, so a project on an
 * unmounted drive is not forgotten silently.
 */
export function RecentProjectList({
  projects,
  onOpen,
  menu = false,
}: {
  projects: RecentProject[];
  onOpen?: () => void;
  menu?: boolean;
}) {
  return (
    <ul class={`recent-list${menu ? " in-menu" : ""}`}>
      {projects.map((project) => (
        <li key={project.path} class={project.exists ? undefined : "missing"}>
          <button
            class="recent-open"
            role={menu ? "menuitem" : undefined}
            title={
              project.exists
                ? project.path
                : `${project.path} (moved or deleted)`
            }
            onClick={() => {
              onOpen?.();
              void openRecent(project);
            }}
          >
            <Icon name="file" size={15} />
            <span class="recent-name">{project.name}</span>
            <span class="recent-folder">
              {project.exists ? parentFolder(project.path) : "missing"}
            </span>
          </button>
          <button
            class="recent-forget"
            title={`Remove ${project.name} from recent projects`}
            aria-label={`Remove ${project.name} from recent projects`}
            onClick={() => void forgetRecent(project.path)}
          >
            <Icon name="close" size={12} />
          </button>
        </li>
      ))}
    </ul>
  );
}
