import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onMounted, onUnmounted } from "vue";
import type { Approval, ExecutionEventPayload, RuntimeEvent, RuntimeInfo } from "@dots/shared-types";
import type { BrowserFrame } from "@/api";
import { useAppsStore } from "@/stores/apps";
import { useApprovalsStore } from "@/stores/approvals";
import { useAutomationStore } from "@/stores/automation";
import { useBrowserStore } from "@/stores/browser";
import { useChatStore } from "@/stores/chat";
import { useContactsStore } from "@/stores/contacts";
import { useDismissalsStore } from "@/stores/dismissals";
import { useOpsStore } from "@/stores/ops";
import { usePoliciesStore } from "@/stores/policies";
import { useDotsStore } from "@/stores/dots";
import { useEnginesStore } from "@/stores/engines";
import { useGroupStore } from "@/stores/group";
import { useMemoriesStore } from "@/stores/memories";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";
import { useTasksStore } from "@/stores/tasks";
import { useTeamsStore } from "@/stores/teams";
import { useUsageStore } from "@/stores/usage";

/** Liga os eventos push do runtime as stores. Usar uma unica vez (App.vue). */
export function useRuntimeEvents() {
  const runtime = useRuntimeStore();
  const dots = useDotsStore();
  const engines = useEnginesStore();
  const settings = useSettingsStore();
  const tasks = useTasksStore();
  const chat = useChatStore();
  const contacts = useContactsStore();
  const approvals = useApprovalsStore();
  const memories = useMemoriesStore();
  const automation = useAutomationStore();
  const ops = useOpsStore();
  const apps = useAppsStore();
  const teams = useTeamsStore();
  const browser = useBrowserStore();
  const dismissals = useDismissalsStore();
  const usage = useUsageStore();
  const group = useGroupStore();
  const policies = usePoliciesStore();
  const unlisteners: UnlistenFn[] = [];

  async function loadAll() {
    await Promise.allSettled([settings.load(), dots.load(), engines.load(), contacts.load(), approvals.load(), tasks.loadActive(), apps.load(), teams.load(), usage.load(), group.load()]);
  }

  async function handle(payload: RuntimeEvent) {
    const p = payload.payload as Record<string, unknown>;
    switch (payload.event) {
      case "dots.changed":
        await Promise.allSettled([dots.load(), contacts.load(), teams.load(), group.loadMembers()]);
        break;
      case "engines.changed":
        await engines.load();
        break;
      case "settings.changed":
        await settings.load();
        break;
      case "task.changed":
        chat.onTaskChanged(p as { id: string; dot_id?: string; status: string });
        if (group.isMember(p.dot_id as string | undefined)) void group.onChanged();
        await Promise.allSettled([tasks.load(), tasks.loadActive(), contacts.load(), tasks.selectedId === p.id ? tasks.refreshSelected() : Promise.resolve()]);
        break;
      case "execution.event":
        chat.onExecutionEvent(payload.payload as ExecutionEventPayload);
        tasks.onExecutionEvent(payload.payload as ExecutionEventPayload);
        group.onExecutionEvent(payload.payload as ExecutionEventPayload);
        break;
      case "approval.requested":
        approvals.onRequested(payload.payload as Approval);
        await contacts.load();
        break;
      case "approval.resolved":
        await Promise.allSettled([approvals.load(), contacts.load()]);
        break;
      case "message.changed":
        await chat.onMessageChanged(p as { conversation_id?: string | null; dot_id?: string });
        await Promise.allSettled([contacts.load(), group.onChanged()]);
        break;
      case "conversation.changed":
        await contacts.load();
        break;
      case "memory.changed":
        if (memories.dotId === p.dot_id) await memories.load();
        break;
      case "runtime.paused":
        await ops.load();
        break;
      case "automation.changed":
        if (automation.dotId === p.dot_id) await automation.load();
        break;
      case "delegation.changed":
        if (tasks.selectedId) await tasks.refreshSelected();
        break;
      case "event.recorded":
        await ops.load();
        break;
      case "teams.changed":
        await Promise.allSettled([teams.load(), dismissals.reloadAll(), group.loadMembers()]);
        break;
      case "learned.changed":
        if (p.dot_id) await policies.loadLearned(p.dot_id as string);
        break;
      case "group.changed":
        await group.onChanged();
        break;
      case "apps.changed":
        await apps.load();
        break;
      case "browser.frame":
        browser.onFrame(payload.payload as BrowserFrame);
        break;
      case "runtime.shutting_down":
        runtime.setDisconnected();
        browser.close();
        break;
    }
  }

  onMounted(async () => {
    unlisteners.push(
      await listen<RuntimeEvent>("runtime:event", ({ payload }) => void handle(payload)),
      await listen<RuntimeInfo>("runtime:connected", async ({ payload }) => {
        runtime.setConnected(payload);
        await runtime.refresh();
        await loadAll();
      }),
      await listen("runtime:disconnected", () => runtime.setDisconnected()),
    );
    await runtime.ensure();
    if (runtime.isConnected) await loadAll();
  });

  onUnmounted(() => unlisteners.forEach((u) => u()));
}
