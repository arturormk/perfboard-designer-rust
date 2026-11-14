/** Minimal stroke icon set (24 × 24 grid, 1.75 stroke), drawn for this app. */
const PATHS = {
  select: "M5 3l13 7.5-5.5 1.5L10 18z",
  wire: "M4 18h5V6h6v6h5 M4 18a1.6 1.6 0 1 0 0 .01 M20 12a1.6 1.6 0 1 0 0 .01",
  rail: "M3 12h18 M6 12v0 M12 12v0 M18 12v0 M6 9.5v5 M12 9.5v5 M18 9.5v5",
  // A strap held down at two holes across a bundle of wires.
  saddle:
    "M3 10h18 M3 14h18 M6 5v3 M18 5v3 M6 6.5h12 M6 19v-3 M18 19v-3 M6 17.5h12",
  bridge: "M6 12h12 M6 12a2.5 2.5 0 1 1 0 .01 M18 12a2.5 2.5 0 1 1 0 .01",
  resize: "M4 9V4h5 M20 15v5h-5 M4 4l6 6 M20 20l-6-6",
  undo: "M9 14L4 9l5-5 M4 9h10a6 6 0 0 1 0 12h-3",
  redo: "M15 14l5-5-5-5 M20 9H10a6 6 0 0 0 0 12h3",
  flip: "M12 3v18 M8 7L4 12l4 5 M16 7l4 5-4 5",
  fit: "M4 9V4h5 M15 4h5v5 M20 15v5h-5 M9 20H4v-5",
  plus: "M12 5v14 M5 12h14",
  minus: "M5 12h14",
  close: "M6 6l12 12 M18 6L6 18",
  check: "M5 12.5l4.5 4.5L19 7",
  trash: "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13",
  rotate: "M20 11a8 8 0 1 0-2.3 5.7 M20 4v7h-7",
  lock: "M6 11h12v9H6z M8.5 11V8a3.5 3.5 0 0 1 7 0v3",
  unlock: "M6 11h12v9H6z M8.5 11V8a3.5 3.5 0 0 1 6.8-1.2",
  route:
    "M5 19c0-8 14-6 14-14 M5 19a1.6 1.6 0 1 0 0 .01 M19 5a1.6 1.6 0 1 0 0 .01",
  eye: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6",
  layers: "M12 3l9 5-9 5-9-5z M3 13l9 5 9-5",
  chip: "M7 7h10v10H7z M9.5 4v3 M14.5 4v3 M9.5 17v3 M14.5 17v3 M4 9.5h3 M4 14.5h3 M17 9.5h3 M17 14.5h3",
  net: "M6 6a2 2 0 1 0 0 .01 M18 6a2 2 0 1 0 0 .01 M12 18a2 2 0 1 0 0 .01 M6 8l6 8 M18 8l-6 8",
  history: "M3 12a9 9 0 1 0 3-6.7 M3 4v5h5 M12 7v5l3 2",
  warning: "M12 4l9 16H3z M12 10v4 M12 17v.01",
  info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18 M12 11v6 M12 7.5v.01",
  error: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18 M9 9l6 6 M15 9l-6 6",
  agent:
    "M12 3l1.8 4.6L18.5 9l-4.7 1.5L12 15l-1.8-4.5L5.5 9l4.7-1.4z M18 15l.9 2.1L21 18l-2.1.9L18 21l-.9-2.1L15 18l2.1-.9z",
  user: "M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M4 21c1-4.5 4-6.5 8-6.5s7 2 8 6.5",
  commit: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M3 12h5 M16 12h5",
  compare: "M4 5h7v14H4z M13 5h7v14h-7z",
  file: "M6 3h8l4 4v14H6z M14 3v4h4",
  folder: "M3 6h6l2 2h10v11H3z",
  save: "M5 4h11l3 3v13H5z M8 4v5h7V4 M8 20v-6h8v6",
  print: "M7 9V4h10v5 M5 9h14v7h-2 M7 16H5V9 M7 13h10v7H7z",
  more: "M5 12v.01 M12 12v.01 M19 12v.01",
  grip: "M9 6v.01 M15 6v.01 M9 12v.01 M15 12v.01 M9 18v.01 M15 18v.01",
  chevron: "M9 6l6 6-6 6",
  chevronDown: "M6 9l6 6 6-6",
  chevronUp: "M6 15l6-6 6 6",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14 M20 20l-4-4",
  target: "M12 3v4 M12 17v4 M3 12h4 M17 12h4 M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6",
  keyboard:
    "M3 7h18v11H3z M6 10v.01 M9 10v.01 M12 10v.01 M15 10v.01 M18 10v.01 M7 14h10",
  refresh:
    "M20 11a8 8 0 0 0-14.7-4 M4 4v4h4 M4 13a8 8 0 0 0 14.7 4 M20 20v-4h-4",
  panelLeft: "M4 4h16v16H4z M9 4v16",
  panelRight: "M4 4h16v16H4z M15 4v16",
  tag: "M3 12V4h8l10 10-8 8z M7.5 7.5v.01",
  spark:
    "M12 2v4 M12 18v4 M4.9 4.9l2.8 2.8 M16.3 16.3l2.8 2.8 M2 12h4 M18 12h4 M4.9 19.1l2.8-2.8 M16.3 7.7l2.8-2.8",
  board:
    "M3 5h18v14H3z M7 9v.01 M11 9v.01 M15 9v.01 M7 13v.01 M11 13v.01 M15 13v.01 M19 9v.01 M19 13v.01",
  component: "M5 7h14v10H5z M8 4v3 M12 4v3 M16 4v3 M8 17v3 M12 17v3 M16 17v3",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({
  name,
  size = 18,
  class: className,
  title,
}: {
  name: IconName;
  size?: number;
  class?: string;
  title?: string;
}) {
  return (
    <svg
      class={`icon${className ? ` ${className}` : ""}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden={title ? undefined : "true"}
      role={title ? "img" : undefined}
    >
      {title && <title>{title}</title>}
      <path d={PATHS[name]} />
    </svg>
  );
}
