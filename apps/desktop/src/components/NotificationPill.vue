<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import Avatar from "@/components/Avatar.vue";
import { useNotificationsStore } from "@/stores/notifications";

/**
 * Balao fixo no topo: avisos que os DOTs mandaram com dots_notify_user. Nao some sozinho: so quando o usuario
 * clica (abrir a conversa do DOT limpa os avisos dele; "Limpar" limpa todos). Fechado mostra o primeiro aviso
 * (ou quantos agentes precisam dele); "Ver" abre a lista completa.
 */
const router = useRouter();
const notifications = useNotificationsStore();
const items = computed(() => notifications.items);
const dots = computed(() => notifications.dotIds);
const first = computed(() => items.value[0] ?? null);
const open = ref(false);
const seen = new Set<string>();

const headline = computed(() => (dots.value.length > 1 ? `${dots.value.length} agentes precisam de você` : `${first.value?.dot_name ?? ""} precisa de você`));
const subline = computed(() => (dots.value.length > 1 ? items.value.map((n) => `${n.dot_name} · ${n.title}`).join(" · ") : summary(first.value)));
/** Avisos alem do primeiro (mesmo DOT): aparecem como "+N" no balao fechado. */
const extra = computed(() => (dots.value.length > 1 ? 0 : items.value.length - 1));
const actionLabel = computed(() => (dots.value.length > 1 ? "Ver" : label(first.value)));

function summary(n: { title: string; detail: string } | null) {
  return n ? [n.title, n.detail].filter(Boolean).join(" · ") : "";
}
function label(n: { action: string } | null) {
  return n?.action || "Ver";
}

watch(items, (list) => {
  const fresh = list.some((n) => !seen.has(n.id));
  list.forEach((n) => seen.add(n.id));
  if (!list.length) open.value = false;
  if (fresh) void signal();
}, { immediate: true });
onUnmounted(() => { open.value = false; });

/** Chama a atencao no Dock se o app nao estiver em foco; o balao fica na tela de qualquer jeito. */
async function signal() {
  try {
    const appWindow = getCurrentWindow();
    if (!(await appWindow.isFocused())) await appWindow.requestUserAttention(UserAttentionType.Informational);
  } catch {
    // Sem Dock (teste ou outra plataforma): o balao ja basta.
  }
}

function primary() {
  if (dots.value.length > 1) open.value = !open.value;
  else if (first.value) void respond(first.value.dot_id);
}

async function respond(dotId: string) {
  open.value = false;
  await notifications.clear({ dotId });
  void router.push(`/chat/${dotId}`);
}

async function clearAll() {
  open.value = false;
  await notifications.clear();
}
</script>

<template>
  <div v-if="first" class="pointer-events-none fixed inset-x-0 top-14 z-50 flex justify-center px-4" role="status" aria-live="polite">
    <div class="pointer-events-auto w-[min(560px,100%)]">
      <div v-if="!open" class="flex items-center gap-3 rounded-full border border-zinc-700/80 bg-zinc-950/95 py-1.5 pl-1.5 pr-1.5 text-sm text-zinc-100 shadow-2xl backdrop-blur" data-testid="notification-pill">
        <button type="button" class="flex min-w-0 flex-1 items-center gap-3 text-left" :title="open ? 'Fechar' : 'Ver os avisos'" @click="open = !open">
          <span class="flex shrink-0 items-center">
            <span v-for="(id, i) in dots.slice(0, 2)" :key="id" class="flex rounded-full ring-2 ring-violet-500" :class="i ? '-ml-2' : ''"><Avatar :name="items.find((n) => n.dot_id === id)?.dot_name ?? '?'" :seed="id" size="sm" /></span>
          </span>
          <span class="min-w-0 flex-1">
            <span class="block truncate font-semibold">{{ headline }}</span>
            <span class="block truncate text-xs text-zinc-400">{{ subline }}</span>
          </span>
        </button>
        <span v-if="extra > 0" class="shrink-0 rounded-full bg-zinc-700 px-2 py-0.5 text-[11px] font-semibold text-zinc-100" :title="`${extra} aviso(s) a mais`">+{{ extra }}</span>
        <button type="button" class="shrink-0 rounded-full bg-violet-500 px-4 py-1.5 text-xs font-semibold text-white hover:bg-violet-400" @click="primary">{{ actionLabel }}</button>
      </div>
      <div v-else class="overflow-hidden rounded-2xl border border-zinc-700/80 bg-zinc-950/95 text-sm text-zinc-100 shadow-2xl backdrop-blur" role="dialog" aria-label="Notificações">
        <div class="flex items-center justify-between px-4 pt-3 pb-1">
          <span class="text-[11px] font-semibold uppercase tracking-wide text-zinc-400">{{ items.length }} {{ items.length === 1 ? "notificação" : "notificações" }}</span>
          <button type="button" class="text-xs text-violet-300 hover:underline" @click="clearAll">Limpar</button>
        </div>
        <ul class="max-h-[60vh] overflow-y-auto pb-1.5">
          <li v-for="n in items" :key="n.id" class="flex items-center gap-3 px-3 py-2 hover:bg-zinc-900/80">
            <span class="flex shrink-0 rounded-full ring-2 ring-violet-500"><Avatar :name="n.dot_name" :seed="n.dot_id" size="sm" /></span>
            <span class="min-w-0 flex-1">
              <span class="block truncate font-semibold">{{ n.dot_name }} precisa de você</span>
              <span class="block truncate text-xs text-zinc-400">{{ summary(n) }}</span>
            </span>
            <button type="button" class="shrink-0 rounded-full px-3 py-1.5 text-xs font-semibold" :class="n.id === first.id ? 'bg-violet-500 text-white hover:bg-violet-400' : 'bg-zinc-800 text-zinc-100 hover:bg-zinc-700'" @click="respond(n.dot_id)">{{ label(n) }}</button>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
