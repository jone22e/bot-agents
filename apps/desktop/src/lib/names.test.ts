import { describe, expect, it } from "vitest";
import { COMMON_NAMES, randomName } from "./names";

describe("randomName", () => {
  it("skips names already in use and the current one", () => {
    const taken = COMMON_NAMES.filter((n) => n !== "Ana" && n !== "Joao").map((n) => n.toUpperCase());
    expect(randomName(taken, "joao", () => 0.99)).toBe("Ana");
  });
  it("still returns a common name when all are taken", () => {
    expect(COMMON_NAMES).toContain(randomName(COMMON_NAMES, "", () => 0));
  });
});
