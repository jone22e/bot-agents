import { defineStore } from "pinia";
import type { AppPaths, LaunchdStatus, RuntimeInfo, RuntimeStatus } from "@dots/shared-types";
import { api, runtimeApi, RuntimeError } from "@/api";

export type ConnectionState = "connecting" | "connected" | "disconnected" | "error";

export const useRuntimeStore = defineStore("runtime", {
  state: () => ({
    connection: "connecting" as ConnectionState,
    info: null as RuntimeInfo | null,
    status: null as RuntimeStatus | null,
    launchd: null as LaunchdStatus | null,
    paths: null as AppPaths | null,
    error: null as string | null,
    busy: false,
  }),
  getters: {
    isConnected: (s) => s.connection === "connected",
  },
  actions: {
    setConnected(info: RuntimeInfo) {
      this.info = info;
      this.connection = "connected";
      this.error = null;
    },
    setDisconnected() {
      this.connection = "disconnected";
      this.status = null;
    },
    async ensure() {
      this.connection = "connecting";
      this.busy = true;
      try {
        const info = await runtimeApi.ensure();
        this.setConnected(info);
        await this.refresh();
      } catch (e) {
        this.connection = "error";
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.busy = false;
      }
    },
    async refresh() {
      try {
        this.status = await api.runtime.status();
        if (this.connection !== "connected") this.connection = "connected";
      } catch (e) {
        if (e instanceof RuntimeError && e.code === "RUNTIME_UNAVAILABLE") this.setDisconnected();
        else this.error = e instanceof Error ? e.message : String(e);
      }
      try {
        this.launchd = await runtimeApi.launchdStatus();
        this.paths = await runtimeApi.appPaths();
      } catch {
        /* informativo */
      }
    },
    async setStartAtLogin(enabled: boolean) {
      try { this.paths = await runtimeApi.setStartAtLogin(enabled); }
      catch (e) { this.error = e instanceof Error ? e.message : String(e); }
    },
    async stop() {
      this.busy = true;
      try {
        await runtimeApi.stop();
        this.setDisconnected();
        this.launchd = await runtimeApi.launchdStatus();
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e);
      } finally {
        this.busy = false;
      }
    },
    async restart() {
      this.busy = true;
      try {
        const info = await runtimeApi.restart();
        this.setConnected(info);
        await this.refresh();
      } catch (e) {
        this.connection = "error";
        this.error = e instanceof Error ? e.message : String(e);
      } finally {
        this.busy = false;
      }
    },
  },
});
