<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import { BellRing, X } from "lucide-vue-next";
import RequestCard from "@/components/RequestCard.vue";
import { useApprovalsStore } from "@/stores/approvals";
import { useDotsStore } from "@/stores/dots";

const router = useRouter();
const approvals = useApprovalsStore();
const dots = useDotsStore();
const requests = computed(() => approvals.pending.filter((a) => dots.byId(a.dot_id)?.role === "supervisor"));
const current = computed(() => requests.value[0] ?? null);
const open = ref(false);
const seen = new Set<string>();

watch(requests, (items) => {
  const newRequest = items.some((a) => !seen.has(a.id));
  items.forEach((a) => seen.add(a.id));
  if (newRequest) open.value = true;
  if (!items.length) open.value = false;
  void signal(items.length, newRequest);
}, { immediate: true });
onUnmounted(() => void signal(0, false));

async function signal(count: number, newRequest: boolean) {
  try {
    const appWindow = getCurrentWindow();
    try { await appWindow.setBadgeCount(count || undefined); } catch { /* Sem badge nesta plataforma. */ }
    if (newRequest && !(await appWindow.isFocused())) {
      await appWindow.requestUserAttention(UserAttentionType.Informational);
    }
  } catch {
    // O alerta dentro do app continua disponivel se o sistema nao suportar o aviso no Dock.
  }
}

function showConversation(dotId: string) {
  open.value = false;
  void router.push(`/chat/${dotId}`);
}

function decideCurrent(approved: boolean, answer?: string) {
  if (current.value) void approvals.decide(current.value.id, approved, answer);
}
</script>

<template>
  <template v-if="current">
    <div class="flex shrink-0 items-center gap-3 border-b border-amber-500/30 bg-amber-950/50 px-4 py-2 text-sm text-amber-100" role="status">
      <BellRing :size="17" class="shrink-0 text-amber-300" />
      <span class="min-w-0 flex-1 truncate"><strong>{{ current.dot_name }}</strong> espera sua {{ current.kind === "question" ? "resposta" : "permissão" }}<template v-if="current.kind === 'question'">: {{ current.summary }}</template></span>
      <span v-if="requests.length > 1" class="shrink-0 text-xs text-amber-300">{{ requests.length }} pendentes</span>
      <button type="button" class="shrink-0 rounded-lg bg-amber-400 px-3 py-1.5 text-xs font-semibold text-zinc-950 hover:bg-amber-300" @click="open = true">Responder</button>
    </div>
    <aside v-if="open" class="fixed right-4 top-24 z-50 max-h-[calc(100vh-7rem)] w-[min(560px,calc(100vw-2rem))] overflow-y-auto rounded-2xl border border-amber-500/40 bg-zinc-900 p-4 shadow-2xl" role="dialog" aria-label="Resposta pendente de gerente">
      <div class="mb-3 flex items-start gap-3">
        <div class="min-w-0 flex-1">
          <h2 class="text-sm font-semibold text-zinc-100">{{ current.dot_name }} precisa da sua {{ current.kind === "question" ? "resposta" : "permissão" }}</h2>
          <p v-if="requests.length > 1" class="mt-0.5 text-xs text-zinc-400">{{ requests.length }} pedidos pendentes · responda um por vez</p>
        </div>
        <button type="button" class="rounded-md p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100" title="Ver depois" aria-label="Ver depois" @click="open = false"><X :size="16" /></button>
      </div>
      <RequestCard :key="current.id" :approval="current" :busy="approvals.deciding !== null" @decide="decideCurrent" />
      <p v-if="approvals.error" class="mt-3 text-xs text-red-300">{{ approvals.error }}</p>
      <button type="button" class="mt-3 text-xs text-violet-300 hover:underline" @click="showConversation(current.dot_id)">Abrir conversa com {{ current.dot_name }}</button>
    </aside>
  </template>
</template>
