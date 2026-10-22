import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

import FlexiBrowserLogin from "./FlexiBrowserLogin.vue";
import { useAppsStore } from "@/stores/apps";

const url = "https://flexi.test/login-external-app?token=tok-1";

describe("login do Flexi no navegador", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("mostra a URL original e permite copia-la quando o sistema abre HTML local", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    useAppsStore().browserLogin = { token: "tok-1", code: "ABC-123", authorize_url: url, expires_at: null, poll_interval_ms: 1000 };

    const wrapper = mount(FlexiBrowserLogin);
    expect((wrapper.get('input[aria-label="Link de autorização do Flexi"]').element as HTMLInputElement).value).toBe(url);
    await wrapper.get("button.btn-secondary:last-child").trigger("click");
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith(url);
    expect(wrapper.get('[role="status"]').text()).toContain("Link copiado");
  });

  it("seleciona o link quando a area de transferencia nao esta disponivel", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
    useAppsStore().browserLogin = { token: "tok-1", code: "ABC-123", authorize_url: url, expires_at: null, poll_interval_ms: 1000 };

    const wrapper = mount(FlexiBrowserLogin);
    await wrapper.get("button.btn-secondary:last-child").trigger("click");
    await flushPromises();
    const input = wrapper.get('input[aria-label="Link de autorização do Flexi"]').element as HTMLInputElement;
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe(url.length);
    expect(wrapper.get('[role="status"]').text()).toContain("Link selecionado");
  });
});
