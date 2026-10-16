import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Estado da atualizacao do app (lado Rust: src-tauri/src/updater.rs). */
export type UpdateState = {
  status: "idle" | "checking" | "available" | "downloading" | "ready" | "error" | "unsupported";
  /** Versao instalada */
  current: string;
  /** Versao nova encontrada */
  version: string | null;
  percent: number | null;
  error: string | null;
  /** Ultima checagem que terminou sem versao nova (ms) */
  checked_at: number | null;
  /** "Atualizar automaticamente" ligado */
  auto: boolean;
};

/** Atualizacao automatica (desenho do ovrgit): o Rust checa, baixa e instala; aqui so o estado e os botoes. */
export const useUpdaterStore = defineStore("updater", {
  state: () => ({ state: null as UpdateState | null, /** Barra "pronto para instalar" fechada (volta com outra versao). */ dismissed: false, started: false }),
  getters: {
    text(s): string {
      const u = s.state;
      if (!u) return "";
      if (u.status === "unsupported") return "Disponível só no app instalado.";
      if (u.status === "checking") return "Procurando versão nova…";
      if (u.status === "available") return `Versão ${u.version} disponível.`;
      if (u.status === "downloading") return `Baixando a versão ${u.version ?? "nova"}… ${u.percent ?? 0}%`;
      if (u.status === "ready") return `Versão ${u.version} pronta para instalar.`;
      if (u.status === "error") return u.error ?? "Não foi possível verificar.";
      return u.checked_at ? "Você está na versão mais recente." : "";
    },
  },
  actions: {
    async init() {
      if (this.started) return;
      this.started = true;
      await listen<UpdateState>("update://state", (e) => this.apply(e.payload));
      try { this.apply(await invoke<UpdateState>("update_state")); } catch { /* sem o lado Rust (testes) */ }
    },
    apply(s: UpdateState) {
      if (s.version !== this.state?.version) this.dismissed = false;
      this.state = s;
    },
    async check() { this.apply(await invoke<UpdateState>("update_check")); },
    async download() { this.apply(await invoke<UpdateState>("update_download")); },
    /** Fecha o app e abre a versao nova. */
    async install() { await invoke("update_install"); },
    async setAuto(auto: boolean) { this.apply(await invoke<UpdateState>("update_set_auto", { auto })); },
  },
});
