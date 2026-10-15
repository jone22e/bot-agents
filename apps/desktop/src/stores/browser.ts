import { defineStore } from "pinia";
import type { BrowserFrame, BrowserStatus } from "@/api";
import { api, RuntimeError } from "@/api";

/** Renova o `watch` antes de o runtime encerrar a transmissao (prazo de 15 s no runtime). */
const HEARTBEAT_MS = 5000;

/** Transmissao ao vivo do navegador proprio de um DOT ("computador do DOT"). */
export const useBrowserStore = defineStore("browser", {
  state: () => ({
    dotId: null as string | null,
    status: null as BrowserStatus | null,
    /** Ultimo quadro como data URL (JPEG). */
    frame: null as string | null,
    width: 0,
    height: 0,
    frames: 0,
    connecting: false,
    error: null as string | null,
    heartbeat: null as ReturnType<typeof setInterval> | null,
  }),
  getters: {
    /** Texto de estado para a interface. */
    phase(state): "connecting" | "idle" | "live" | "error" {
      if (state.error) return "error";
      if (state.frame) return "live";
      if (state.connecting) return "connecting";
      return "idle";
    },
  },
  actions: {
    /** Abre a transmissao do DOT (inicia o navegador se preciso) e mantem o heartbeat. */
    async open(dotId: string) {
      if (this.dotId && this.dotId !== dotId) await this.close();
      this.dotId = dotId;
      this.error = null;
      this.connecting = true;
      await this.renew();
      if (this.heartbeat) clearInterval(this.heartbeat);
      this.heartbeat = setInterval(() => void this.renew(), HEARTBEAT_MS);
    },
    async renew() {
      if (!this.dotId) return;
      try {
        const st = await api.browser.watch(this.dotId);
        this.status = st;
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
        this.connecting = false;
      }
    },
    async refreshStatus() {
      if (!this.dotId) return;
      try {
        this.status = await api.browser.status(this.dotId);
      } catch {
        /* estado e informativo */
      }
    },
    onFrame(f: BrowserFrame) {
      if (f.dot_id !== this.dotId) return;
      this.frame = `data:image/jpeg;base64,${f.data}`;
      this.width = f.width;
      this.height = f.height;
      this.frames += 1;
      this.connecting = false;
      if (this.frames % 10 === 1) void this.refreshStatus();
    },
    /** Encerra a transmissao e limpa o estado. */
    async close() {
      if (this.heartbeat) clearInterval(this.heartbeat);
      this.heartbeat = null;
      const id = this.dotId;
      this.dotId = null;
      this.frame = null;
      this.frames = 0;
      this.status = null;
      this.connecting = false;
      this.error = null;
      if (id) await api.browser.unwatch(id).catch(() => undefined);
    },
  },
});
