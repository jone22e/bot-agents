import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useTeamsStore } from "./teams";

const teams = [
  { id: "t1", name: "Marketplaces", description: "", manager_dot_id: "boss", member_ids: ["sub", "pub"], created_at: "", updated_at: "" },
  { id: "t2", name: "Mercado Livre", description: "", manager_dot_id: "sub", member_ids: ["a1"], created_at: "", updated_at: "" },
  { id: "t3", name: "Shopee", description: "", manager_dot_id: null, member_ids: [], created_at: "", updated_at: "" },
];

describe("teams store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("resolves who manages and who belongs where", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => (a.method === "teams.list" ? teams : null));
    const s = useTeamsStore();
    await s.load();
    expect(s.managedBy("sub")?.name).toBe("Mercado Livre");
    expect(s.memberOf("sub")?.name).toBe("Marketplaces");
    expect(s.memberOf("boss")).toBeNull();
    expect(s.withoutManager.map((t) => t.name)).toEqual(["Shopee"]);
  });

  it("addMember reloads and keeps the error when rejected", async () => {
    const calls: string[] = [];
    invoke.mockImplementation(async (_c: string, a: { method: string }) => {
      calls.push(a.method);
      if (a.method === "teams.add_member") throw { code: "VALIDATION", message: "isso criaria um ciclo" };
      return teams;
    });
    const s = useTeamsStore();
    expect(await s.addMember("t2", "boss")).toBeNull();
    expect(s.error).toContain("ciclo");
    // nao recarrega quando o runtime recusa
    expect(calls).toEqual(["teams.add_member"]);
  });
});
