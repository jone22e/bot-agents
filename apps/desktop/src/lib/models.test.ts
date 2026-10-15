import { describe, expect, it } from "vitest";
import type { EngineModel } from "@dots/shared-types";
import { modelGroups, modelLabel } from "./models";

const codex: EngineModel[] = [
  { id: "gpt-6.1-sol", label: "GPT-6.1 Sol", description: "Modelo de trabalho mais recente." },
  { id: "gpt-6-astra", label: "GPT-6 Astra", description: null },
];
const agy: EngineModel[] = [
  { id: "gemini-3.8-flash-high", label: "Gemini 3.8 Flash (High)", description: null },
  { id: "claude-opus-5-5-low", label: "Claude Opus 5.5 (Low)", description: null },
  { id: "gemini-3.8-flash-low", label: "Gemini 3.8 Flash (Low)", description: null },
];

describe("modelGroups", () => {
  it("claude usa a lista fixa, sem depender do catalogo", () => {
    const g = modelGroups("claude");
    expect(g[0].items.map((m) => m.id)).toEqual(["sonnet", "haiku", "opus", "fable"]);
    expect(g.some((x) => x.label.startsWith("Opus"))).toBe(true);
  });
  it("codex lista o catalogo do CLI na ordem recebida", () => {
    expect(modelGroups("codex")).toEqual([]);
    const g = modelGroups("codex", codex);
    expect(g).toHaveLength(1);
    expect(g[0].items.map((m) => m.id)).toEqual(["gpt-6.1-sol", "gpt-6-astra"]);
    expect(g[0].items[0].description).toBe("Modelo de trabalho mais recente.");
    expect(g[0].items[1].description).toBeUndefined();
  });
  it("antigravity agrupa pela familia do nome", () => {
    const g = modelGroups("agy", agy);
    expect(g.map((x) => x.label)).toEqual(["Gemini", "Claude"]);
    expect(g[0].items.map((m) => m.id)).toEqual(["gemini-3.8-flash-high", "gemini-3.8-flash-low"]);
  });
});

describe("modelLabel", () => {
  it("usa o nome do catalogo e cai para o id", () => {
    expect(modelLabel("codex", "gpt-6.1-sol", codex)).toBe("GPT-6.1 Sol");
    expect(modelLabel("codex", "gpt-6.1-sol")).toBe("gpt-6.1-sol");
    expect(modelLabel("claude", "claude-opus-4-6")).toBe("Opus 4.6");
    expect(modelLabel("agy", "")).toBe("");
  });
});
