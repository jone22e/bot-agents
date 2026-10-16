import { invoke } from "@tauri-apps/api/core";
import type { AppPaths, DotsError, LaunchdStatus, RuntimeInfo } from "@dots/shared-types";

/** Erro normalizado vindo do runtime ou do lado Rust do app. */
export class RuntimeError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.name = "RuntimeError";
  }
}

function normalize(e: unknown): RuntimeError {
  if (e instanceof RuntimeError) return e;
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    const d = e as DotsError;
    return new RuntimeError(d.code, d.message);
  }
  return new RuntimeError("INTERNAL", typeof e === "string" ? e : String(e));
}

/** Chamada generica ao runtime via comando Tauri `ipc_call`. */
export async function ipcCall<T>(method: string, params: unknown = {}): Promise<T> {
  try {
    return await invoke<T>("ipc_call", { method, params });
  } catch (e) {
    throw normalize(e);
  }
}

export const runtimeApi = {
  info: () => invoke<RuntimeInfo | null>("runtime_info").catch((e) => { throw normalize(e); }),
  ensure: () => invoke<RuntimeInfo>("runtime_ensure").catch((e) => { throw normalize(e); }),
  stop: () => invoke<void>("runtime_stop").catch((e) => { throw normalize(e); }),
  restart: () => invoke<RuntimeInfo>("runtime_restart").catch((e) => { throw normalize(e); }),
  launchdStatus: () => invoke<LaunchdStatus>("runtime_launchd_status").catch((e) => { throw normalize(e); }),
  appPaths: () => invoke<AppPaths>("app_paths").catch((e) => { throw normalize(e); }),
  setStartAtLogin: (enabled: boolean) => invoke<AppPaths>("runtime_set_start_at_login", { enabled }).catch((e) => { throw normalize(e); }),
};

/** Midia do compositor: ditado e miniaturas (comandos do lado Rust do app). */
export const mediaApi = {
  /** WAV mono 16 kHz -> texto, transcrito neste Mac. */
  transcribe: (wav: Uint8Array) => invoke<string>("transcribe_audio", wav).catch((e) => { throw normalize(e); }),
  /** Miniatura (data URL) de uma imagem; null se nao for imagem. */
  imagePreview: (path: string) => invoke<string | null>("image_preview", { path }).catch(() => null),
};
