import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useNotificationsStore } from "./notifications";

import { notice } from "./notifications.fixture";

describe("notifications store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("carrega as pendentes e agrupa por DOT", async () => {
    invoke.mockResolvedValueOnce([notice("1", "carol", "Carolina", "AGY com ERROR"), notice("2", "rafa", "Rafael", "3 notas"), notice("3", "carol", "Carolina", "Outra")]);
    const s = useNotificationsStore();
    await s.load();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.list", params: { pending_only: true, limit: 100 } });
    expect(s.dotIds).toEqual(["carol", "rafa"]);
    expect(s.countByDot.get("carol")).toBe(2);
  });

  it("limpa uma, as de um DOT ou todas, sem esperar o runtime para sumir da tela", async () => {
    const s = useNotificationsStore();
    s.items = [notice("1", "carol", "Carolina", "a"), notice("2", "rafa", "Rafael", "b"), notice("3", "carol", "Carolina", "c")];
    invoke.mockResolvedValue({ cleared: 1 });
    await s.clear({ dotId: "carol" });
    expect(s.items.map((n) => n.id)).toEqual(["2"]);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: "1" } });
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: "3" } });
    invoke.mockClear();
    await s.clear();
    expect(s.items).toEqual([]);
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: null } });
  });

  it("se o runtime falhar ao limpar, recarrega para nao esconder o aviso", async () => {
    const s = useNotificationsStore();
    s.items = [notice("1", "carol", "Carolina", "a")];
    invoke.mockRejectedValueOnce({ code: "IO", message: "sem socket" }).mockResolvedValueOnce([notice("1", "carol", "Carolina", "a")]);
    await s.clear({ id: "1" });
    expect(s.items).toHaveLength(1);
    expect(s.error).toBe("sem socket");
  });
});
