// Catalogo de modelos por assistente (mesma origem do ovrgit/Ovseer):
// - Claude Code: lista fixa abaixo (o CLI nao lista modelos).
// - Codex e Antigravity: o proprio CLI informa na deteccao (`EngineDetection.models`:
//   `~/.codex/models_cache.json` e `agy models`).
import type { EngineKind, EngineModel } from "@dots/shared-types";

export interface ModelOption { id: string; label: string; description?: string }
export interface ModelGroup { label: string; items: ModelOption[] }

/** Apelidos do Claude Code: seguem sempre a versao mais nova da familia. */
export const CLAUDE_ALIASES: ModelOption[] = [
  { id: "sonnet", label: "Sonnet", description: "equilibrio (recomendado)" },
  { id: "haiku", label: "Haiku", description: "mais rapido" },
  { id: "opus", label: "Opus", description: "mais capaz" },
  { id: "fable", label: "Fable", description: "o mais capaz" },
];

/** Versoes exatas por familia, da mais nova para a mais antiga. */
export const CLAUDE_FAMILIES: { family: string; models: ModelOption[] }[] = [
  { family: "Fable", models: [
    { id: "claude-fable-5-1", label: "Fable 5.1" },
    { id: "claude-fable-5", label: "Fable 5" },
  ] },
  { family: "Opus", models: [
    { id: "claude-opus-5-5", label: "Opus 5.5" },
    { id: "claude-opus-4-8", label: "Opus 4.8" },
    { id: "claude-opus-4-7", label: "Opus 4.7" },
    { id: "claude-opus-4-6", label: "Opus 4.6" },
    { id: "claude-opus-4-5-20251101", label: "Opus 4.5" },
    { id: "claude-opus-4-1-20250805", label: "Opus 4.1" },
    { id: "claude-opus-4-20250514", label: "Opus 4" },
  ] },
  { family: "Sonnet", models: [
    { id: "claude-sonnet-5", label: "Sonnet 5" },
    { id: "claude-sonnet-4-6", label: "Sonnet 4.6" },
    { id: "claude-sonnet-4-5-20250929", label: "Sonnet 4.5" },
    { id: "claude-sonnet-4-20250514", label: "Sonnet 4" },
  ] },
  { family: "Haiku", models: [{ id: "claude-haiku-4-5-20251001", label: "Haiku 4.5" }] },
];

/**
 * Grupos do seletor para um assistente. `catalog` sao os modelos que o CLI informou na deteccao
 * (Codex: na ordem do app; Antigravity: agrupados pela familia que aparece no nome, ex. "Gemini", "Claude").
 */
export function modelGroups(engine: EngineKind | "", catalog: readonly EngineModel[] = []): ModelGroup[] {
  if (engine === "claude") {
    return [
      { label: "Sempre a versao mais recente", items: CLAUDE_ALIASES },
      ...CLAUDE_FAMILIES.map((f) => ({ label: `${f.family} · versao fixa`, items: f.models })),
    ];
  }
  if (!engine || !catalog.length) return [];
  const items: ModelOption[] = catalog.map((m) => ({ id: m.id, label: m.label, description: m.description ?? undefined }));
  if (engine === "codex") return [{ label: "Catalogo do Codex", items }];
  const groups = new Map<string, ModelOption[]>();
  for (const m of items) {
    const family = m.label.split(" ")[0] || "Outros";
    groups.set(family, [...(groups.get(family) ?? []), m]);
  }
  return [...groups].map(([label, items]) => ({ label, items }));
}

/** Nome curto e legivel de um modelo (ex.: "claude-opus-4-6" -> "Opus 4.6"; ids do catalogo usam o nome informado). */
export function modelLabel(engine: EngineKind | "", id: string, catalog: readonly EngineModel[] = []): string {
  if (!id) return "";
  const known = modelGroups(engine, catalog).flatMap((g) => g.items).find((m) => m.id === id);
  if (known) return known.label;
  const m = /^claude-([a-z]+)-(\d+)(?:-(\d+))?/.exec(id);
  if (m) return `${m[1][0].toUpperCase()}${m[1].slice(1)} ${m[2]}${m[3] ? `.${m[3]}` : ""}`;
  return id;
}

export const MODEL_PLACEHOLDER: Record<EngineKind, string> = { claude: "ex.: claude-opus-5-5", codex: "ex.: gpt-5-codex", agy: "ex.: gemini-3.8-flash-high" };
