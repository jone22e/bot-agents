import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { ago, resetsIn, useUsageStore } from "./usage";

const win = (pct: number) => ({ pct, resets_at: null });
const prov = (week: number | null, five: number | null) => ({ five_hour: five === null ? null : win(five), week: week === null ? null : win(week), plan: null, at: new Date().toISOString(), error: null });

describe("usage store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("loads usage and updates, averages the week and lists missing CLIs", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => {
      if (a.method === "usage.get") return { claude: prov(49, 76), codex: prov(6, null), agy: null };
      if (a.method === "cli.updates") return { claude: { current: "2.1.300", latest: "2.1.301", available: true }, codex: { current: "0.162.1", latest: "0.162.1", available: false }, agy: null };
      return null;
    });
    const s = useUsageStore();
    await s.load();
    expect(s.rows.map((r) => r.id)).toEqual(["claude", "codex"]);
    expect(s.weekAvg).toBe(27.5);
    expect(s.missing).toEqual(["agy"]);
    expect(s.visible).toBe(true);
    expect(s.hasUpdate).toBe(true);
  });

  it("update marks the CLI as updated when nothing newer remains", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => {
      if (a.method === "usage.get") return { claude: prov(1, 1), codex: null, agy: null };
      if (a.method === "cli.updates") return { claude: { current: "2.1.300", latest: "2.1.301", available: true }, codex: null, agy: null };
      if (a.method === "cli.update") return { current: "2.1.301", latest: "2.1.301", available: false };
      return null;
    });
    const s = useUsageStore();
    await s.load();
    await s.update("claude");
    expect(s.updated.claude).toBe("2.1.301");
    expect(s.updates?.claude?.available).toBe(false);
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "cli.update", params: { kind: "claude" } });
  });

  it("install failure keeps the error for the row", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => {
      if (a.method === "cli.install") throw { code: "ENGINE", message: "A instalacao terminou com codigo 1." };
      return { claude: null, codex: null, agy: null };
    });
    const s = useUsageStore();
    await s.load();
    await s.install("codex");
    expect(s.errors.codex).toContain("codigo 1");
    expect(s.installed.codex).toBeUndefined();
  });

  it("formats resets and age", () => {
    const now = Date.parse("2026-10-15T10:00:00Z");
    expect(resetsIn({ resets_at: "2026-10-15T10:31:00Z" }, now)).toBe("zera em 31 min");
    expect(resetsIn({ resets_at: "2026-10-18T09:00:00Z" }, now)).toBe("zera em 2 d 23 h");
    expect(resetsIn(null, now)).toBe("");
    expect(ago("2026-10-15T09:59:50Z", now)).toBe("agora");
    expect(ago("2026-10-15T08:00:00Z", now)).toBe("ha 2 h");
  });
});
