import { describe, expect, it } from "vitest";
import { avatarUri } from "./avatar";

describe("avatarUri", () => {
  it("gera sempre a mesma imagem para o mesmo DOT e imagens diferentes para DOTs diferentes", () => {
    const a = avatarUri("dot-a");
    expect(a.startsWith("data:image/svg+xml")).toBe(true);
    expect(avatarUri("dot-a")).toBe(a);
    expect(avatarUri("dot-b")).not.toBe(a);
  });
});
