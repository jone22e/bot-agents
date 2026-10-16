import { describe, expect, it } from "vitest";
import { THEMES, findTheme, mix, themeVars } from "./themes";

describe("themes", () => {
  it("default has no overrides and unknown ids fall back to it", () => {
    expect(themeVars(findTheme("dots"))).toEqual({});
    expect(findTheme("nao-existe").id).toBe("dots");
    expect(new Set(THEMES.map((t) => t.id)).size).toBe(THEMES.length);
  });
  it("maps the palette onto the tailwind scale the app uses", () => {
    const v = themeVars(findTheme("dracula"));
    expect(v["--color-zinc-950"]).toBe("#21222c");
    expect(v["--color-zinc-100"]).toBe("#f8f8f2");
    expect(v["--color-indigo-600"]).toBe("#bd93f9");
    expect(v["--color-amber-500"]).toBe("#f1fa8c");
    expect(v["--accent"]).toBe("#bd93f9");
    expect(v["--sidebar"]).toBe(mix("#21222c", "#282a36", 0.5));
  });
  it("light themes keep the neutral scale semantic (950 = background)", () => {
    const v = themeVars(findTheme("github-light"));
    expect(v["--color-zinc-950"]).toBe("#f6f8fa");
    expect(v["--color-zinc-100"]).toBe("#1f2328");
  });
  it("mix is linear in rgb", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mix("#102030", "#102030", 0.3)).toBe("#102030");
  });
});
