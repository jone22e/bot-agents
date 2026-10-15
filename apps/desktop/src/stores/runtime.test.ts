import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useRuntimeStore } from "./runtime";

const info = { pid: 1, version: "0.1.0", proto: 1, socket_path: "/s", token_path: "/t", started_at: "x", mode: "detached" };
const status = { pid: 1, version: "0.1.0", protocol_version: 1, started_at: "x", uptime_seconds: 5, data_dir: "/d", socket_path: "/s", schema_version: 1, mode: "detached", connected_clients: 1 };

describe("runtime store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("ensure connects and refreshes status", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "runtime_ensure") return info;
      if (cmd === "ipc_call") return status;
      if (cmd === "runtime_launchd_status") return { supported: true, plist_installed: false, loaded: false, pid: null, last_exit_status: null };
      if (cmd === "app_paths") return { data_dir: "/d", logs_dir: "/d/logs", bin_dir: "/d/bin", launch_agent_plist: "/p", bundled_dotsd: null, dev_mode: true };
      throw new Error("unexpected " + cmd);
    });
    const s = useRuntimeStore();
    await s.ensure();
    expect(s.connection).toBe("connected");
    expect(s.status?.schema_version).toBe(1);
    expect(s.launchd?.supported).toBe(true);
  });

  it("ensure failure sets error state", async () => {
    invoke.mockRejectedValueOnce({ code: "RUNTIME_UNAVAILABLE", message: "nao subiu" });
    const s = useRuntimeStore();
    await s.ensure();
    expect(s.connection).toBe("error");
    expect(s.error).toBe("nao subiu");
  });

  it("refresh marks disconnected when runtime unavailable", async () => {
    const s = useRuntimeStore();
    s.setConnected(info);
    invoke.mockRejectedValueOnce({ code: "RUNTIME_UNAVAILABLE", message: "x" }).mockResolvedValue({});
    await s.refresh();
    expect(s.connection).toBe("disconnected");
  });
});
