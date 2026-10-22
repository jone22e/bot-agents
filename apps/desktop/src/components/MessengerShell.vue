<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Activity, BarChart3, Blocks, Check, ChevronDown, ClipboardCheck, LayoutGrid, Plus, Search, Settings, Users } from "lucide-vue-next";
import Avatar from "@/components/Avatar.vue";
import DotContextMenu from "@/components/DotContextMenu.vue";
import Heartbeat from "@/components/Heartbeat.vue";
import UsageChip from "@/components/UsageChip.vue";
import { toneOf } from "@/lib/heartbeat";
import ResizeHandle from "@/components/ResizeHandle.vue";
import { useColumnWidth } from "@/composables/useColumnWidth";
import { useDotMenu } from "@/composables/useDotMenu";
import { useDragReorder } from "@/composables/useDragReorder";
import { ACCOUNT_CONNECTION, useAppsStore } from "@/stores/apps";
import { useApprovalsStore } from "@/stores/approvals";
import { useSettingsStore } from "@/stores/settings";
import { useContactsStore } from "@/stores/contacts";
import { useGroupStore } from "@/stores/group";
import { useNotificationsStore } from "@/stores/notifications";
import { useRuntimeStore } from "@/stores/runtime";
import { useTeamsStore } from "@/stores/teams";

const route = useRoute();
const router = useRouter();
const contacts = useContactsStore();
const apps = useAppsStore();
const teams = useTeamsStore();
const group = useGroupStore();
const notifications = useNotificationsStore();
const approvals = useApprovalsStore();
type Contact = (typeof contacts.items)[number];
/** DOT com aviso pendente (dots_notify_user): linha e avatar destacados ate o usuario clicar nele. */
const notified = (c: Contact) => notifications.countByDot.has(c.dot.id);
/** Abrir a conversa de um DOT e o clique que limpa os avisos dele. */
function openDot(c: Contact) {
  if (notified(c)) void notifications.clear({ dotId: c.dot.id });
  go(`/chat/${c.dot.id}`);
}
/**
 * Lista agrupada por equipe: o gerente primeiro, depois os funcionarios (compactos: o usuario fala com o gerente,
 * que responde as perguntas deles). Quem gerencia uma equipe aparece so nela.
 */
const sections = computed<{ key: string; title: string | null; subtitle: string | null; managerId: string | null; rows: Contact[] }[]>(() => {
  const byId = new Map(contacts.filtered.map((c) => [c.dot.id, c]));
  const managers = new Set(teams.items.map((t) => t.manager_dot_id).filter((x): x is string => !!x));
  const placed = new Set<string>();
  const out: { key: string; title: string | null; subtitle: string | null; managerId: string | null; rows: Contact[] }[] = [];
  for (const t of teams.ordered) {
    const rows: Contact[] = [];
    const m = t.manager_dot_id ? byId.get(t.manager_dot_id) : undefined;
    if (m) { rows.push(m); placed.add(m.dot.id); }
    for (const id of t.member_ids) {
      const c = byId.get(id);
      if (c && !managers.has(id) && !placed.has(id)) { rows.push(c); placed.add(id); }
    }
    const parent = t.manager_dot_id ? teams.memberOf(t.manager_dot_id) : null;
    if (rows.length) out.push({ key: t.id, title: t.name, subtitle: parent ? `em ${parent.name}` : null, managerId: t.manager_dot_id ?? null, rows });
  }
  const rest = contacts.filtered.filter((c) => !placed.has(c.dot.id));
  if (rest.length) out.push({ key: "none", title: out.length ? "Sem equipe" : null, subtitle: null, managerId: null, rows: rest });
  return out;
});
/** Funcionario de uma equipe com gerente: linha compacta. */
const compact = (s: { managerId: string | null }, c: Contact) => s.managerId !== null && c.dot.id !== s.managerId;
const statusOf = (c: Contact) => c.dot.status === "paused" ? "paused" : c.state === "attention" ? "waiting" : c.state === "busy" ? "working" : c.done ? "done" : "idle";
const stateText = (c: Contact) => c.dot.status === "paused" && !c.state_text.startsWith("Pausado") ? `Pausado · ${c.state_text}` : c.state_text;
/** Cargo curto na linha compacta: "Agente de Google Ads: Publicacao" -> "Publicacao" (a equipe ja diz a area). */
const shortTitle = (t: string) => { const i = t.indexOf(":"); return i > 0 ? t.slice(i + 1).trim() : t; };
const runtime = useRuntimeStore();
const settings = useSettingsStore();
const menu = ref(false);
/** Dropdown ao lado de "Visao geral": Central de aprovacoes e Avaliacao de desempenho. */
const hub = ref(false);
/** Botao direito sobre um DOT: acoes rapidas (pausar, retomar, duplicar, arquivar). */
const dotMenu = useDotMenu();
/** Arrastar o titulo de uma equipe para cima ou para baixo reordena os grupos (a ordem vale tambem na visao geral). */
const drag = useDragReorder("y", (from, to, before) => { void teams.reorder(from, to, before); });
/** Conta do usuario no Flexi (vinculada pelo "Logar com Flexi"): a foto dela aparece na bolinha "EU". */
const account = computed(() => apps.flexi.find((c) => c.id === settings.items[ACCOUNT_CONNECTION]) ?? null);
watch(
  () => [account.value?.id ?? null, account.value?.status] as const,
  ([id, status]) => { if (!id) apps.photo = null; else if (status === "connected") void apps.loadPhoto(id); },
  { immediate: true },
);
/** Largura da lista de contatos, ajustavel pela alca a direita dela. */
const sidebar = useColumnWidth("sidebar", 288, 220, 480);
const currentDot = computed(() => (route.params.dotId as string | undefined) ?? null);
/** Linha do grupo geral: ultima mensagem (de quem) ou os gerentes que estao nele. */
const groupPreview = computed(() => {
  const m = group.last;
  if (!m) return group.members.length ? `${group.members.length} gerente(s): ${group.members.map((d) => d.name).join(", ")}` : "Sem gerentes ainda";
  return `${m.role === "user" ? "Voce" : m.dot_name ?? "?"}: ${(m.content || "…").replace(/\s+/g, " ")}`;
});

function preview(c: (typeof contacts.items)[number]) {
  if (c.state !== "idle") return c.state_text;
  if (!c.last_message) return "Sem mensagens ainda";
  return (c.last_role === "user" ? "Voce: " : "") + c.last_message.replace(/\s+/g, " ");
}
function when(iso: string) {
  const d = new Date(iso);
  const today = new Date();
  return d.toDateString() === today.toDateString() ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : d.toLocaleDateString([], { day: "2-digit", month: "2-digit" });
}
function go(path: string) {
  menu.value = false;
  hub.value = false;
  router.push(path);
}
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden bg-zinc-950" @click="menu = false; hub = false">
    <header class="relative flex h-12 shrink-0 items-center gap-2 border-b border-zinc-800/80 pr-4 pl-24" style="background: var(--panel)" data-tauri-drag-region="deep">
      <span class="text-[11px] font-semibold tracking-[0.16em] text-zinc-400">DOTS</span>
      <div class="absolute left-1/2 flex -translate-x-1/2 items-center gap-0.5" data-tauri-drag-region="false">
        <button class="flex h-8 items-center gap-2 rounded-full px-3 text-sm font-medium hover:bg-zinc-800 hover:text-zinc-100" :class="route.name === 'overview' ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-400'" title="Visão geral dos agentes" @click="go('/overview')"><LayoutGrid :size="15" /> Visão geral</button>
        <div class="relative">
          <button class="flex h-8 items-center gap-1 rounded-full px-2 text-sm font-medium hover:bg-zinc-800 hover:text-zinc-100" :class="hub || route.name === 'approvals' || route.name === 'performance' ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-400'" title="Central de aprovações e avaliação de desempenho" aria-haspopup="menu" :aria-expanded="hub" data-testid="hub-toggle" @click.stop="hub = !hub; menu = false">
            <ChevronDown :size="15" class="transition-transform" :class="hub ? 'rotate-180' : ''" />
            <span v-if="approvals.pending.length" class="rounded-full bg-amber-500 px-1.5 text-[10px] font-semibold text-zinc-950" title="Decisões pendentes">{{ approvals.pending.length }}</span>
          </button>
          <div v-if="hub" role="menu" class="absolute left-1/2 top-[calc(100%+6px)] z-50 w-[360px] -translate-x-1/2 rounded-xl border border-zinc-700 bg-zinc-900 py-1 text-sm shadow-2xl" @click.stop>
            <button role="menuitem" class="flex w-full items-start gap-3 px-3 py-2.5 text-left hover:bg-zinc-800" data-testid="approvals-link" @click="go('/approvals')">
              <ClipboardCheck :size="17" class="mt-0.5 shrink-0 text-zinc-400" />
              <span class="min-w-0 flex-1">
                <span class="flex items-center gap-2 font-medium text-zinc-100">Central de aprovações<span v-if="approvals.pending.length" class="rounded-full bg-amber-500 px-1.5 text-[10px] font-semibold text-zinc-950">{{ approvals.pending.length }}</span></span>
                <span class="block text-xs leading-snug text-zinc-400">Uma fila única de decisões pendentes, com justificativa, impacto previsto, evidências e botões para aprovar, rejeitar ou solicitar revisão.</span>
              </span>
            </button>
            <button role="menuitem" class="flex w-full items-start gap-3 px-3 py-2.5 text-left hover:bg-zinc-800" data-testid="performance-link" @click="go('/performance')">
              <BarChart3 :size="17" class="mt-0.5 shrink-0 text-zinc-400" />
              <span class="min-w-0 flex-1">
                <span class="block font-medium text-zinc-100">Avaliação de desempenho</span>
                <span class="block text-xs leading-snug text-zinc-400">Taxa de sucesso, reincidência, custo por operação, tempo de resolução, ações revertidas e tarefas que exigiram intervenção humana.</span>
              </span>
            </button>
          </div>
        </div>
      </div>
      <span class="flex-1" />
      <UsageChip opens="down" data-tauri-drag-region="false" />
      <div class="relative" data-tauri-drag-region="false">
        <button class="flex h-9 w-9 items-center justify-center overflow-hidden rounded-full bg-zinc-800 text-xs font-semibold text-zinc-100 hover:bg-zinc-700" :title="account ? `${account.user_name ?? account.email} · Menu` : 'Menu'" @click.stop="menu = !menu">
          <img v-if="apps.photo" :src="apps.photo" alt="" class="h-full w-full object-cover" />
          <template v-else>EU</template>
        </button>
        <div v-if="menu" class="absolute right-0 top-[calc(100%+6px)] z-50 w-52 rounded-xl border border-zinc-700 bg-zinc-900 py-1 text-sm shadow-2xl" @click.stop>
          <button class="flex w-full items-center justify-between px-3 py-2 text-left hover:bg-zinc-800" @click="go('/activities')"><span class="flex items-center gap-2"><Activity :size="15" /> Atividades</span><span v-if="contacts.attention" class="rounded-full bg-amber-500 px-1.5 text-[10px] font-semibold text-zinc-950">{{ contacts.attention }}</span></button>
          <button class="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-zinc-800" @click="go('/teams')"><Users :size="15" /> Equipes</button>
          <button class="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-zinc-800" @click="go('/settings')"><Settings :size="15" /> Configuracoes</button>
          <button class="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-zinc-800" @click="go('/engines')"><Blocks :size="15" /> Conectar aplicativos</button>
          <div class="my-1 border-t border-zinc-800"></div>
          <p class="px-3 py-1.5 text-xs text-zinc-500">{{ runtime.isConnected ? "Servico ativo" : "Servico parado" }}</p>
        </div>
      </div>
    </header>
    <div class="flex min-h-0 flex-1 overflow-hidden">
    <aside class="flex shrink-0 flex-col border-r border-zinc-800/80" :style="{ width: `${sidebar.width.value}px`, backgroundColor: 'var(--sidebar)' }">
      <div class="flex items-center justify-end gap-2 px-3 pt-3 pb-2" data-tauri-drag-region>
        <div class="relative flex-1">
          <Search :size="14" class="pointer-events-none absolute left-3 top-2.5 text-zinc-500" />
          <input v-model="contacts.query" class="h-9 w-full rounded-full border border-zinc-800 bg-zinc-900 pl-8 pr-3 text-sm text-zinc-100 placeholder-zinc-500 focus:border-zinc-600 focus:outline-none" placeholder="Buscar" />
        </div>
        <button class="flex h-9 w-9 items-center justify-center rounded-full bg-zinc-800 text-zinc-100 hover:bg-zinc-700" title="Novo DOT" @click="go('/dots/new')"><Plus :size="18" /></button>
      </div>

      <div class="sidebar-scroll flex-1 overflow-y-auto px-2">
        <button v-if="runtime.isConnected && apps.needsAttention.length" class="m-2 w-[calc(100%-1rem)] rounded-lg bg-amber-950/40 p-3 text-left text-xs text-amber-200 hover:bg-amber-950/60" @click="go('/engines')">{{ apps.needsAttention[0].label }}: {{ apps.needsAttention[0].status === 'mfa_pending' ? 'aguardando o codigo 2FA' : 'precisa entrar de novo' }}. <span class="underline">Resolver</span></button>
        <p v-if="!runtime.isConnected" class="m-2 rounded-lg bg-amber-950/40 p-3 text-xs text-amber-200">Servico parado. <button class="underline" @click="runtime.ensure()">Ligar</button></p>
        <p v-else-if="!contacts.loading && !contacts.items.length" class="m-2 rounded-lg bg-zinc-900 p-3 text-xs text-zinc-400">Nenhum DOT ainda. Toque em + para criar o primeiro.</p>
        <button v-if="runtime.isConnected && contacts.items.length" class="mb-0.5 flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left hover:bg-zinc-800/70" :class="route.name === 'group' ? 'bg-zinc-800' : ''" @click="go('/group')">
          <span class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-zinc-700 text-zinc-100"><Users :size="18" /></span>
          <span class="min-w-0 flex-1">
            <span class="flex items-baseline justify-between gap-2">
              <span class="truncate text-sm font-medium text-zinc-100">Grupo geral</span>
              <span v-if="group.last" class="shrink-0 text-[11px] text-zinc-500">{{ when(group.last.created_at) }}</span>
            </span>
            <span class="block truncate text-xs text-zinc-500">{{ groupPreview }}</span>
          </span>
        </button>
        <div v-for="s in sections" :key="s.key" class="team-section" :class="drag.classOf(s.key)" :data-drag-id="s.key !== 'none' ? s.key : undefined">
        <p v-if="s.title" class="mt-3 mb-1 flex items-baseline gap-1.5 px-3 text-[11px] font-semibold uppercase tracking-wide text-zinc-500" :class="s.key !== 'none' ? 'cursor-grab select-none' : ''" :title="s.key !== 'none' ? 'Arraste para reordenar as equipes' : undefined" @pointerdown="s.key !== 'none' && drag.start($event, s.key)"><span class="truncate">{{ s.title }}</span><span v-if="s.subtitle" class="truncate font-normal normal-case tracking-normal text-zinc-600">{{ s.subtitle }}</span></p>
        <template v-for="c in s.rows" :key="c.dot.id">
        <button v-if="compact(s, c)" class="mb-0.5 flex w-full items-center gap-2.5 rounded-lg py-1.5 pl-7 pr-3 text-left hover:bg-zinc-800/70" :class="[currentDot === c.dot.id ? 'bg-zinc-800' : '', notified(c) ? 'notified' : '']" @click="openDot(c)" @contextmenu="dotMenu.open($event, c.dot)">
          <span class="sidebar-avatar compact" :class="statusOf(c)"><Avatar :name="c.dot.name" :seed="c.dot.id" size="sm" /><span v-if="statusOf(c) === 'done'" class="status-marker"><Check :size="9" :stroke-width="3" /></span><span v-if="notified(c)" class="notify-marker" title="Precisa de você" /></span>
          <span class="min-w-0 flex-1">
            <span class="flex items-baseline gap-1.5">
              <span class="max-w-[60%] shrink-0 truncate text-[13px] text-zinc-200">{{ c.dot.name }}</span>
              <span v-if="c.title" class="min-w-0 flex-1 truncate text-[11px] text-zinc-500">{{ shortTitle(c.title) }}</span>
            </span>
            <span v-if="c.dot.status === 'paused' || c.state !== 'idle'" class="block truncate text-[11px]" :class="c.dot.status === 'paused' || c.state === 'attention' ? 'text-amber-300' : 'text-sky-300'">{{ stateText(c) }}</span>
          </span>
          <Heartbeat v-if="toneOf(c.pulse) !== 'idle'" :tone="toneOf(c.pulse)" />
          <span v-if="c.pending_requests" class="rounded-full bg-amber-500 px-1.5 text-[10px] font-semibold text-zinc-950">{{ c.pending_requests }}</span>
        </button>
        <button v-else class="mb-0.5 flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left hover:bg-zinc-800/70" :class="[currentDot === c.dot.id ? 'bg-zinc-800' : '', notified(c) ? 'notified' : '']" @click="openDot(c)" @contextmenu="dotMenu.open($event, c.dot)">
          <span class="sidebar-avatar" :class="statusOf(c)"><Avatar :name="c.dot.name" :seed="c.dot.id" /><span v-if="statusOf(c) === 'done'" class="status-marker"><Check :size="9" :stroke-width="3" /></span><span v-if="notified(c)" class="notify-marker" title="Precisa de você" /></span>
          <span class="min-w-0 flex-1">
            <span class="flex items-baseline justify-between gap-2">
              <span class="truncate text-sm font-medium text-zinc-100">{{ c.dot.name }}</span>
              <span class="shrink-0 text-[11px] text-zinc-500">{{ when(c.last_at) }}</span>
            </span>
            <span v-if="c.title" class="block truncate text-[11px] text-zinc-400">{{ c.title }}</span>
            <span class="flex items-center gap-2">
              <span class="min-w-0 flex-1 truncate text-xs" :class="c.dot.status === 'paused' || c.state === 'attention' ? 'text-amber-300' : c.state === 'busy' ? 'text-sky-300' : 'text-zinc-500'">{{ c.dot.status === 'paused' ? stateText(c) : preview(c) }}</span>
              <Heartbeat v-if="toneOf(c.pulse) !== 'idle'" :tone="toneOf(c.pulse)" />
            </span>
          </span>
          <span v-if="c.pending_requests" class="rounded-full bg-amber-500 px-1.5 text-[10px] font-semibold text-zinc-950">{{ c.pending_requests }}</span>
        </button>
        </template>
        </div>
      </div>

    </aside>
    <ResizeHandle grows="right" @resize="sidebar.resize" @reset="sidebar.reset" />

    <main class="flex min-w-0 flex-1 flex-col">
      <slot />
    </main>
    </div>
    <DotContextMenu />
  </div>
</template>

<style scoped>
.sidebar-avatar { position: relative; display: inline-flex; height: 2.75rem; width: 2.75rem; flex-shrink: 0; align-items: center; justify-content: center; border: 2px solid #52525b; border-radius: 9999px; background: #27272a; }
.sidebar-avatar.compact { height: 2rem; width: 2rem; }
.sidebar-avatar.working { border-color: #8b5cf6; }
.sidebar-avatar.working::after { content: ""; position: absolute; inset: -4px; border-radius: 9999px; border: 3px solid rgb(139 92 246 / 0.25); border-top-color: rgb(139 92 246 / 0.8); animation: spin 1.1s linear infinite; }
.sidebar-avatar.done { border-color: #22c55e; }
.sidebar-avatar.waiting { border-color: #f97316; }
.sidebar-avatar.paused { border-color: #fbbf24; }
/* DOT com aviso pendente: anel violeta na linha e ponto no canto do avatar (some quando o usuario clica nele). */
.notified { box-shadow: inset 0 0 0 1px rgb(139 92 246 / 0.7); background: rgb(139 92 246 / 0.08); }
.notify-marker { position: absolute; right: -3px; top: -3px; height: 12px; width: 12px; border: 2px solid #202027; border-radius: 9999px; background: #8b5cf6; }
.sidebar-avatar.compact .notify-marker { height: 10px; width: 10px; right: -2px; top: -2px; }
.status-marker { position: absolute; right: -4px; bottom: -4px; display: flex; height: 14px; width: 14px; align-items: center; justify-content: center; border: 2px solid #202027; border-radius: 9999px; background: #22c55e; color: white; }
@keyframes spin { to { transform: rotate(360deg); } }
.team-section { border-top: 2px solid transparent; border-bottom: 2px solid transparent; }
.team-section.drag-source { opacity: 0.45; }
.team-section.drop-before { border-top-color: #8b5cf6; }
.team-section.drop-after { border-bottom-color: #8b5cf6; }
@media (prefers-reduced-motion: reduce) { .sidebar-avatar.working::after { animation: none; } }
</style>
