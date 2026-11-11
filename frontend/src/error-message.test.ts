import { describe, expect, it } from "vitest";

import { errorMessage } from "./error-message";

describe("errorMessage", () => {
  it("extracts messages from structured desktop errors", () => {
    expect(
      errorMessage({
        code: "session.operation_failed",
        message: "component definition id conflicts with its stored snapshot",
      }),
    ).toBe("component definition id conflicts with its stored snapshot");
  });

  it("preserves string and Error messages", () => {
    expect(errorMessage("plain failure")).toBe("plain failure");
    expect(errorMessage(new Error("typed failure"))).toBe("typed failure");
  });
});
