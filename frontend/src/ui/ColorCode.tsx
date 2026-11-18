import { useId } from "preact/hooks";
import type { Band } from "../api/project";
import { BAND_COLORS, OUTLINED_BANDS, bandRects } from "../model/bands";

/** What each band of a five-band code stands for. */
const ROLES = ["1st digit", "2nd digit", "3rd digit", "Multiplier", "1%"];

/**
 * The outline of an axial resistor in a 200 × 40 box: rounded end caps that
 * bulge out, joined by a narrower waist.
 */
const BODY_PATH = [
  "M 46 6",
  "C 54 6 56 11 63 11",
  "L 137 11",
  "C 144 11 146 6 154 6",
  "C 162 6 168 12 168 20",
  "C 168 28 162 34 154 34",
  "C 146 34 144 29 137 29",
  "L 63 29",
  "C 56 29 54 34 46 34",
  "C 38 34 32 28 32 20",
  "C 32 12 38 6 46 6 Z",
].join(" ");

/** A resistor drawn lying down, with its bands, as a reference to match. */
function ResistorDrawing({
  bands,
  width = 200,
}: {
  bands: readonly Band[];
  width?: number;
}) {
  const clip = `resistor-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  // Bands are laid out along the body and cut to its outline, so the ones on
  // an end cap stand taller, as on a real part.
  const rects = bandRects(
    bands,
    { x: 32, y: 6, width: 136, height: 28 },
    "deg0",
  );
  return (
    <svg
      class="resistor-drawing"
      viewBox="0 0 200 40"
      width={width}
      height={(width * 40) / 200}
      aria-hidden="true"
    >
      <defs>
        <clipPath id={clip}>
          <path d={BODY_PATH} />
        </clipPath>
      </defs>
      <line class="resistor-lead" x1={0} y1={20} x2={200} y2={20} />
      <path class="resistor-body" d={BODY_PATH} />
      <g clip-path={`url(#${clip})`}>
        {rects.map((rect, index) => (
          <rect
            key={index}
            x={rect.x}
            y={rect.y}
            width={rect.width}
            height={rect.height}
            fill={BAND_COLORS[rect.band]}
          />
        ))}
        <path class="resistor-shine" d="M 36 13 H 164" />
      </g>
      <path class="resistor-outline" d={BODY_PATH} />
    </svg>
  );
}

/**
 * A resistor's colour code as a reference card: the resistor drawn with its
 * bands, then each band named with what it stands for. Read from the end
 * away from the gap; the last band, after the gap, is the 1% tolerance.
 */
export function ColorCode({
  bands,
  value,
}: {
  bands: readonly Band[];
  value: string;
}) {
  if (!bands.length) return null;
  const zero = bands.length === 1;
  return (
    <div
      class="color-code"
      role="group"
      aria-label={`Colour code for ${value}: ${bands.join(", ")}`}
    >
      <ResistorDrawing bands={bands} />
      <ol class="color-code-bands">
        {bands.map((band, index) => (
          <li key={index}>
            <i
              class={`color-code-swatch${OUTLINED_BANDS.has(band) ? " outlined" : ""}`}
              style={{ background: BAND_COLORS[band] }}
            />
            <span class="color-code-name">{band}</span>
            <span class="color-code-role">
              {zero ? "Zero-ohm link" : ROLES[index]}
            </span>
          </li>
        ))}
      </ol>
      {!zero && (
        <p class="field-note">
          Read from the left. The band after the gap is the tolerance.
        </p>
      )}
    </div>
  );
}

/** A small drawing and the colour names, for tooltips. */
export function ColorCodeStrip({ bands }: { bands: readonly Band[] }) {
  if (!bands.length) return null;
  return (
    <div
      class="color-code-strip"
      aria-label={`Colour bands: ${bands.join(", ")}`}
    >
      <ResistorDrawing bands={bands} width={140} />
      <span class="color-code-names">{bands.join(" · ")}</span>
    </div>
  );
}

/** One line per resistor: name, value, a small drawing and the colours. */
export function ColorCodeRow({
  name,
  value,
  bands,
}: {
  name: string;
  value: string;
  bands: readonly Band[];
}) {
  return (
    <div class="color-code-row">
      <span class="color-code-part">
        <strong>{name}</strong> {value}
      </span>
      <ResistorDrawing bands={bands} width={96} />
      <span class="color-code-names">{bands.join(" · ")}</span>
    </div>
  );
}
