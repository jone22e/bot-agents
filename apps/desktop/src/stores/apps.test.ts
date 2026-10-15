import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useAppsStore } from "./apps";

describe("apps store (conexoes com o Flexi)", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("connect sends the input, reloads the list and exposes attention state", async () => {
    const calls: { method: string; params?: unknown }[] = [];
    invoke.mockImplementation(async (_c: string, a: { method: string; params?: unknown }) => {
      calls.push(a);
      if (a.method === "apps.flexi.connect") return { id: "c1", kind: "flexi", status: "mfa_pending" };
      if (a.method === "apps.list") return [{ id: "c1", kind: "flexi", status: "mfa_pending", label: "Flexi · x" }];
      return null;
    });
    const s = useAppsStore();
    const c = await s.connect({ label: null, base_url: null, server: "teste", email: "a@b.c", senha: "s", remember_password: true });
    expect(c?.status).toBe("mfa_pending");
    expect(calls[0].params).toMatchObject({ email: "a@b.c", remember_password: true });
    expect(s.flexi).toHaveLength(1);
    expect(s.flexiConnected).toBe(false);
    expect(s.needsAttention.map((c) => c.id)).toEqual(["c1"]);
    expect(s.error).toBeNull();
  });

  it("keeps the runtime error message when a call is rejected", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => {
      if (a.method === "apps.flexi.mfa") throw { code: "UNAUTHORIZED", message: "codigo invalido" };
      return [];
    });
    const s = useAppsStore();
    expect(await s.mfa("c1", "000")).toBeNull();
    expect(s.error).toContain("codigo invalido");
    expect(s.busy).toBe(false);
  });
});
