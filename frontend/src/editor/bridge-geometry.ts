const TERMINAL_RADIUS = 0.36;
const SPAN_INSET = 0.4;

function coordinate(value: number): string {
  const rounded = Math.round(value * 10_000) / 10_000;
  return String(Object.is(rounded, -0) ? 0 : rounded);
}

export function bridgeConductorPaths(
  start: { x: number; y: number },
  end: { x: number; y: number },
): { terminals: string; span: string } {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const length = Math.hypot(dx, dy);
  if (length === 0) return { terminals: "", span: "" };

  const alongX = dx / length;
  const alongY = dy / length;
  const acrossX = -alongY;
  const acrossY = alongX;
  const radius = Math.min(TERMINAL_RADIUS, length * 0.4);
  const spanInset = Math.min(SPAN_INSET, length / 2);
  const point = (x: number, y: number) => `${coordinate(x)} ${coordinate(y)}`;

  const startTop = point(
    start.x - acrossX * radius,
    start.y - acrossY * radius,
  );
  const startControl = point(
    start.x + alongX * radius * 2,
    start.y + alongY * radius * 2,
  );
  const startBottom = point(
    start.x + acrossX * radius,
    start.y + acrossY * radius,
  );
  const endTop = point(end.x - acrossX * radius, end.y - acrossY * radius);
  const endControl = point(
    end.x - alongX * radius * 2,
    end.y - alongY * radius * 2,
  );
  const endBottom = point(end.x + acrossX * radius, end.y + acrossY * radius);
  const spanStart = point(
    start.x + alongX * spanInset,
    start.y + alongY * spanInset,
  );
  const spanEnd = point(end.x - alongX * spanInset, end.y - alongY * spanInset);

  return {
    terminals: `M ${startTop} Q ${startControl} ${startBottom} M ${endTop} Q ${endControl} ${endBottom}`,
    span: `M ${spanStart} L ${spanEnd}`,
  };
}
