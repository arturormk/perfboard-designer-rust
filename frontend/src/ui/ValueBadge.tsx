import type { ValueReading } from "../api/project";
import { BAND_COLORS, OUTLINED_BANDS } from "../model/bands";

/**
 * A part's value with its resistor colour code drawn as swatches, or its
 * capacitor code. Each swatch names its colour for screen readers and on
 * hover.
 */
export function ValueBadge({
  value,
  compact = false,
}: {
  value: ValueReading;
  compact?: boolean;
}) {
  const bands = value.bands ?? [];
  return (
    <span class={`value-badge${compact ? " compact" : ""}`}>
      <span class="value-text">{value.display}</span>
      {bands.length > 0 && (
        <span
          class="value-bands"
          role="img"
          aria-label={`Colour bands: ${bands.join(", ")}`}
        >
          {bands.map((band, index) => (
            <i
              key={index}
              class={`value-band${OUTLINED_BANDS.has(band) ? " outlined" : ""}${
                bands.length > 1 && index === bands.length - 1
                  ? " tolerance"
                  : ""
              }`}
              style={{ background: BAND_COLORS[band] }}
              title={band}
            />
          ))}
        </span>
      )}
      {value.code && <span class="value-code">code {value.code}</span>}
    </span>
  );
}
