# Dupont Jumper

A two-pin jumper with male Dupont-style contacts on the standard 2.54 mm
perfboard pitch. The compact default footprint places the pins in adjacent
holes and can be rotated in any cardinal direction.

Use it to represent a removable two-contact jumper or a male-to-male link. The
two pins are passive, so their electrical purpose is determined by the nets
assigned in the design.

The part is a `jumper`: each placed copy can be set to **Open** or
**Shunted** in the inspector. A shunted jumper joins its two pins, so when both
are on the same net they count as connected without a wire. When they are on
two different nets, the shunt joins those nets while it is fitted, which makes
the part usable as a selector. Each net is still routed on its own copper.
