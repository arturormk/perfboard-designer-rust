/** Small hand-built design shared by the model and canvas tests. */
import type { ComponentDefinition } from "../api/project";
import type { Design } from "./geometry";

export const resistor: ComponentDefinition = {
  schema_version: 1,
  id: "test.resistor",
  definition_version: "1",
  name: "Resistor",
  description: "",
  body: { x: 0.635, y: -1.27, width: 3.81, height: 2.54 },
  allowed_rotations: ["deg0", "deg90"],
  pins: [
    { id: "a", number: "1", name: "1", x: 0, y: 0 },
    { id: "b", number: "2", name: "2", x: 2, y: 0 },
  ],
};

export const bridge: ComponentDefinition = {
  schema_version: 1,
  kind: "bridge",
  id: "test.bridge",
  definition_version: "1",
  name: "Bridge",
  description: "",
  body: { x: 0, y: 0, width: 2.54, height: 0.1 },
  allowed_rotations: ["deg0", "deg90", "deg180", "deg270"],
  pins: [
    { id: "a", number: "A", name: "end A", x: 0, y: 0 },
    { id: "b", number: "B", name: "end B", x: 1, y: 0 },
  ],
};

export function testDesign(): Design {
  return {
    board: { name: "Test", holes_x: 10, holes_y: 8, pitch_mm: 2.54 },
    component_definitions: { [resistor.id]: resistor, [bridge.id]: bridge },
    components: {
      r1: {
        id: "r1",
        definition_id: resistor.id,
        name: "R1",
        position: { x: 1, y: 1 },
        rotation: "deg0",
      },
      r2: {
        id: "r2",
        definition_id: resistor.id,
        name: "R2",
        position: { x: 1, y: 4 },
        rotation: "deg90",
      },
      r10: {
        id: "r10",
        definition_id: resistor.id,
        name: "R10",
        position: { x: 6, y: 1 },
        rotation: "deg0",
      },
      b1: {
        id: "b1",
        definition_id: bridge.id,
        name: "Bridge 1",
        position: { x: 3, y: 1 },
        rotation: "deg0",
      },
    },
    nets: {
      gnd: {
        id: "gnd",
        name: "GND",
        state: "accepted",
        pins: [
          { component_id: "r1", pin_id: "a" },
          { component_id: "r2", pin_id: "a" },
          { component_id: "r10", pin_id: "b" },
        ],
      },
      sda: {
        id: "sda",
        name: "I2C_SDA",
        state: "proposed",
        pins: [{ component_id: "r1", pin_id: "b" }],
      },
    },
    wires: {
      w1: {
        id: "w1",
        net_id: "gnd",
        start: { kind: "pin", component_id: "r1", pin_id: "a" },
        end: { kind: "pin", component_id: "r2", pin_id: "a" },
        side: "solder",
        color: "black",
        vertices: [
          { x2: 2, y2: 2 },
          { x2: 2, y2: 8 },
        ],
      },
    },
  } as Design;
}
