# Pin

A generic one-pin connector on the standard 2.54 mm perfboard pitch. Use it as
a stand-in for a single Dupont-style header or socket contact, one side of a
two-pin jumper, one position in a larger connector, or a wire that leaves the
board.

Place multiple instances on adjacent holes to represent multi-pin connectors.
Give each instance a descriptive name, such as `Power input`, `J1 pin 3`, or
`Remote sensor signal`, so its role remains clear in the design.

The component deliberately has no access region or directional constraint.
Add those constraints to a more specific component definition when the real
connector's orientation or clearance matters.
