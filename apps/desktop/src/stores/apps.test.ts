import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
const openUrl = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (...a: unknown[]) => openUrl(...a) }));

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
  it("loginWithBrowser opens the Flexi, polls until authorized and saves the account", async () => {
    vi.useFakeTimers();
    const calls: { method?: string; params?: unknown }[] = [];
    const opened: unknown[] = [];
    openUrl.mockImplementation(async (url: string) => { opened.push(url); });
    let polls = 0;
    invoke.mockImplementation(async (_c: string, a: { method?: string; params?: unknown }) => {
      calls.push(a);
      if (a.method === "apps.flexi.browser_start") return { token: "tok-1", code: "ABC-123", authorize_url: "https://flexi.test/login-external-app?token=tok-1", expires_at: null, poll_interval_ms: 1000 };
      if (a.method === "apps.flexi.browser_poll") return ++polls < 2 ? { status: "pending" } : { status: "authenticated", connection: { id: "c9", kind: "flexi", status: "connected" } };
      if (a.method === "apps.list") return [{ id: "c9", kind: "flexi", status: "connected" }];
      if (a.method === "settings.set") return { key: "account.flexi_connection_id", value: "c9" };
      if (a.method === "apps.flexi.photo") return { data_url: "data:image/png;base64,AAAA" };
      return null;
    });
    const s = useAppsStore();
    const pending = s.loginWithBrowser();
    await vi.advanceTimersByTimeAsync(0);
    expect(s.browserLogin?.code).toBe("ABC-123");
    expect(opened).toEqual(["https://flexi.test/login-external-app?token=tok-1"]);
    await vi.advanceTimersByTimeAsync(2500);
    const c = await pending;
    vi.useRealTimers();
    expect(c?.id).toBe("c9");
    expect(s.browserLogin).toBeNull();
    expect(calls.find((x) => x.method === "settings.set")?.params).toMatchObject({ key: "account.flexi_connection_id", value: "c9" });
    await Promise.resolve();
    expect(s.photo).toBe("data:image/png;base64,AAAA");
  });

  it("loginWithBrowser stops with a message when the user denies in the browser", async () => {
    vi.useFakeTimers();
    invoke.mockImplementation(async (_c: string, a: { method?: string }) => {
      if (a.method === "apps.flexi.browser_start") return { token: "tok-2", code: "XYZ-999", authorize_url: "https://flexi.test/x", expires_at: null, poll_interval_ms: 1000 };
      if (a.method === "apps.flexi.browser_poll") return { status: "denied" };
      return null;
    });
    const s = useAppsStore();
    const pending = s.loginWithBrowser();
    await vi.advanceTimersByTimeAsync(1500);
    expect(await pending).toBeNull();
    vi.useRealTimers();
    expect(s.error).toContain("recusado");
    expect(s.browserLogin).toBeNull();
  });
});
