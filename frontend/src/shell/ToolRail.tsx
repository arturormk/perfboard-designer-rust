import { placing, side, tool, type Tool } from "../state/store";
import { Icon, type IconName } from "../ui/Icon";

const TOOLS: Array<{ id: Tool; icon: IconName; label: string; key: string }> = [
  { id: "select", icon: "select", label: "Select and move", key: "V" },
  { id: "wire", icon: "wire", label: "Draw wire", key: "W" },
  { id: "bridge", icon: "bridge", label: "Place bridge", key: "B" },
  { id: "rail", icon: "rail", label: "Draw rail", key: "L" },
  { id: "saddle", icon: "saddle", label: "Tie down wires (saddle)", key: "S" },
  { id: "resize", icon: "resize", label: "Resize board", key: "" },
];

export function activateTool(next: Tool): void {
  placing.value = null;
  tool.value = next;
  // Bridges are solder-side links; show the face they live on.
  if (next === "bridge") side.value = "solder";
}

export function ToolRail() {
  return (
    <nav class="tool-rail" aria-label="Tools">
      {TOOLS.map((item) => (
        <button
          key={item.id}
          class={`tool${tool.value === item.id && !placing.value ? " on" : ""}`}
          aria-pressed={tool.value === item.id}
          title={`${item.label}${item.key ? ` (${item.key})` : ""}`}
          aria-label={item.label}
          onClick={() => activateTool(item.id)}
        >
          <Icon name={item.icon} size={20} />
          {item.key && <span class="tool-key">{item.key}</span>}
        </button>
      ))}
      <div class="rail-spacer" />
      <button
        class="tool"
        title="Flip board (F)"
        aria-label="Flip to the other board side"
        onClick={() =>
          (side.value = side.value === "component" ? "solder" : "component")
        }
      >
        <Icon name="flip" size={20} />
      </button>
    </nav>
  );
}
