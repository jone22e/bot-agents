<script setup lang="ts">
// Grupo geral: o usuario escreve uma vez e todos os gerentes recebem (cada um na propria conversa);
// as respostas deles aparecem aqui com o nome de quem respondeu. Mesmo desenho da conversa (ChatView).
// Digitar @ abre a lista de gerentes: os marcados respondem; os outros recebem so para ciencia.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import type { Dot, GroupMessage, Message } from "@dots/shared-types";
import { ArrowUp, Users } from "lucide-vue-next";
import Avatar from "@/components/Avatar.vue";
import ChatBubble from "@/components/ChatBubble.vue";
import Heartbeat from "@/components/Heartbeat.vue";
import { toneOf } from "@/lib/heartbeat";
import { useContactsStore } from "@/stores/contacts";
import { useGroupStore } from "@/stores/group";
import { useRuntimeStore } from "@/stores/runtime";

const router = useRouter();
const group = useGroupStore();
const contacts = useContactsStore();
const runtime = useRuntimeStore();
const draft = ref("");
const thread = ref<HTMLElement | null>(null);
const box = ref<HTMLTextAreaElement | null>(null);

/** A bolha da conversa recebe uma Message; a resposta em andamento mostra o texto parcial. */
function asMessage(m: GroupMessage): Message {
  const live = m.task_id ? group.live[m.task_id] : undefined;
  return { id: m.id, conversation_id: "", role: m.role, content: live && !m.content ? live : m.content, task_id: m.task_id, execution_id: null, metadata: m.metadata, created_at: m.created_at };
}

type Row = { kind: "date"; key: string; label: string } | { kind: "msg"; key: string; g: GroupMessage; m: Message };
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  let last = "";
  for (const g of group.messages) {
    const d = new Date(g.created_at);
    const day = d.toDateString();
    if (day !== last) {
      last = day;
      const label = day === new Date().toDateString() ? "Hoje" : day === new Date(Date.now() - 86400000).toDateString() ? "Ontem" : d.toLocaleDateString("pt-BR", { day: "2-digit", month: "short" });
      out.push({ kind: "date", key: `d-${day}`, label });
    }
    out.push({ kind: "msg", key: g.id, g, m: asMessage(g) });
  }
  return out;
});
const mentionsOf = (g: GroupMessage) => ((g.metadata as { mentions?: { name: string }[] } | null)?.mentions ?? []).map((m) => m.name);
const titleOf = (dotId: string | null) => (dotId ? contacts.byDot(dotId)?.title ?? null : null);
const working = computed(() => group.members.filter((d) => contacts.byDot(d.id)?.state === "busy"));

async function load() {
  if (runtime.isConnected) await group.load();
}
onMounted(() => {
  group.open = true;
  load();
});
onUnmounted(() => (group.open = false));
watch(() => runtime.isConnected, (v) => v && load());

let stick = true;
function onScroll() {
  const el = thread.value;
  if (el) stick = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
}
function scrollToEnd(force = false) {
  if (!force && !stick) return;
  nextTick(() => {
    const el = thread.value;
    if (el) el.scrollTop = el.scrollHeight;
  });
}
watch(() => [group.messages.length, Object.values(group.live).join("").length], () => scrollToEnd());
onMounted(() => scrollToEnd(true));

// Marcacao com @: lista dos gerentes enquanto o texto antes do cursor termina em "@algo".
const mention = ref<{ start: number; query: string } | null>(null);
const mentionIdx = ref(0);
const norm = (t: string) => t.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
const mentionList = computed(() => {
  if (!mention.value) return [];
  const q = norm(mention.value.query);
  const hit = group.members.filter((d) => norm(d.name).includes(q));
  return [...hit.filter((d) => norm(d.name).startsWith(q)), ...hit.filter((d) => !norm(d.name).startsWith(q))];
});
function detectMention() {
  const el = box.value;
  if (!el) return;
  const caret = el.selectionStart ?? draft.value.length;
  const m = draft.value.slice(0, caret).match(/(?:^|\s)@([^\s@]*)$/);
  mention.value = m ? { start: caret - m[1]!.length - 1, query: m[1]! } : null;
  mentionIdx.value = 0;
}
function pickMention(d: Dot) {
  const el = box.value;
  if (!el || !mention.value) return;
  const caret = el.selectionStart ?? draft.value.length;
  const insert = `@${d.name} `;
  draft.value = draft.value.slice(0, mention.value.start) + insert + draft.value.slice(caret);
  const pos = mention.value.start + insert.length;
  mention.value = null;
  nextTick(() => { el.focus(); el.setSelectionRange(pos, pos); autosize(); });
}
const escapeRe = (t: string) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
/** Gerentes marcados no texto (@Nome inteiro, sem continuar em outra letra). */
const mentioned = computed(() => group.members.filter((d) => new RegExp(`(?:^|\\s)@${escapeRe(d.name)}(?![\\p{L}\\p{N}_])`, "u").test(draft.value)));
function onInput() {
  autosize();
  detectMention();
}

async function send() {
  const body = draft.value.trim();
  if (!body || group.sending) return;
  const ids = mentioned.value.map((d) => d.id);
  draft.value = "";
  mention.value = null;
  nextTick(autosize);
  scrollToEnd(true);
  await group.send(body, ids);
  await contacts.load();
}
function onKey(e: KeyboardEvent) {
  if (mention.value && mentionList.value.length && !e.isComposing) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = mentionList.value.length;
      mentionIdx.value = (mentionIdx.value + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
      return;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      pickMention(mentionList.value[mentionIdx.value]!);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      mention.value = null;
      return;
    }
  }
  if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
  e.preventDefault();
  send();
}
function autosize() {
  const el = box.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
}
</script>

<template>
  <div class="chat flex h-full min-w-0 flex-col">
    <header class="flex flex-col items-center gap-2 py-3" data-tauri-drag-region>
      <span class="flex items-center gap-2 rounded-full bg-zinc-800/90 px-3 py-1.5 text-sm font-medium text-zinc-100"><Users :size="15" /> Grupo geral</span>
      <div v-if="group.members.length" class="flex max-w-full flex-wrap justify-center gap-1.5 px-4">
        <button v-for="d in group.members" :key="d.id" class="flex items-center gap-1.5 rounded-full border border-zinc-800 px-2 py-0.5 text-[11px] text-zinc-300 hover:bg-zinc-800" :title="`Abrir a conversa com ${d.name}`" @click="router.push(`/chat/${d.id}`)">
          <Avatar :name="d.name" size="sm" :state="contacts.byDot(d.id)?.state" />
          <span class="max-w-[160px] truncate">{{ d.name }}</span>
          <Heartbeat v-if="toneOf(contacts.byDot(d.id)?.pulse ?? 'idle') !== 'idle'" :tone="toneOf(contacts.byDot(d.id)?.pulse ?? 'idle')" />
        </button>
      </div>
    </header>
    <p v-if="group.error" class="mx-auto mb-2 rounded-md bg-red-950/40 px-3 py-1 text-xs text-red-200">{{ group.error }}</p>

    <main ref="thread" class="thread" @scroll="onScroll">
      <div v-if="!rows.length" class="empty">
        <span class="flex h-12 w-12 items-center justify-center rounded-full bg-zinc-800 text-zinc-200"><Users :size="22" /></span>
        <h2>Grupo geral</h2>
        <p v-if="group.members.length">Escreva uma vez e todos os gerentes recebem: {{ group.members.map((d) => d.name).join(", ") }}. Cada um responde aqui.</p>
        <p v-else>Ainda nao ha gerentes. Crie uma equipe com gerente em Equipes.</p>
      </div>
      <template v-for="r in rows" :key="r.key">
        <p v-if="r.kind === 'date'" class="day">{{ r.label }}</p>
        <div v-else-if="r.g.role === 'dot'" class="from">
          <button class="who" :title="`Abrir a conversa com ${r.g.dot_name}`" @click="r.g.dot_id && router.push(`/chat/${r.g.dot_id}`)">
            <Avatar :name="r.g.dot_name ?? '?'" size="sm" />
            <strong>{{ r.g.dot_name }}</strong>
            <span v-if="titleOf(r.g.dot_id)" class="ellipsis">{{ titleOf(r.g.dot_id) }}</span>
          </button>
          <ChatBubble :message="r.m" />
        </div>
        <ChatBubble v-else :message="r.m" :mentions="mentionsOf(r.g)" />
      </template>
      <p v-if="working.length" class="progress"><span class="spinner tiny" /> {{ working.map((d) => d.name).join(", ") }} {{ working.length > 1 ? "estao" : "esta" }} trabalhando…</p>
    </main>

    <footer class="composer">
      <ul v-if="mention && mentionList.length" class="mentions" role="listbox">
        <li v-for="(d, i) in mentionList" :key="d.id" role="option" :aria-selected="i === mentionIdx" :class="{ on: i === mentionIdx }" @mousedown.prevent="pickMention(d)" @mouseenter="mentionIdx = i">
          <Avatar :name="d.name" size="sm" />
          <strong>{{ d.name }}</strong>
          <span v-if="titleOf(d.id)" class="ellipsis">{{ titleOf(d.id) }}</span>
        </li>
      </ul>
      <textarea
        ref="box"
        v-model="draft"
        rows="1"
        :disabled="!group.members.length"
        :placeholder="group.members.length ? `Mensagem para todos os gerentes (${group.members.length}) · @ para marcar quem deve responder` : 'Sem gerentes no grupo'"
        @input="onInput"
        @click="detectMention"
        @keydown="onKey"
        @keyup.left="detectMention"
        @keyup.right="detectMention"
        @blur="mention = null"
      />
      <div class="row">
        <span v-if="mentioned.length" class="hint">Só {{ mentioned.map((d) => `@${d.name}`).join(", ") }} {{ mentioned.length > 1 ? "respondem" : "responde" }}; os outros recebem para ciência.</span>
        <span class="spacer" />
        <button type="button" class="icon send primary" title="Enviar (Enter)" :disabled="!draft.trim() || group.sending || !group.members.length" @click="send"><ArrowUp :size="16" /></button>
      </div>
    </footer>
  </div>
</template>

<!-- Estilos globais da conversa (.chat, .md...) vem do ChatView; aqui so o que muda no grupo -->
<style scoped>
.thread { flex: 1; overflow-y: auto; overflow-x: hidden; padding: 20px 22px 12px; display: flex; flex-direction: column; gap: 22px; }
.empty { margin: auto; max-width: 520px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 14px; }
.empty h2 { margin: 0; font-size: 17px; font-weight: 600; letter-spacing: -0.01em; }
.empty p { margin: 0; font-size: 12.5px; line-height: 1.5; color: var(--faint); }
.day { margin: 0; text-align: center; font-size: 11px; color: var(--faint); }
.from { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.who { display: inline-flex; align-items: center; gap: 6px; align-self: flex-start; max-width: 100%; padding: 0; background: transparent; border: 0; font-size: 12px; color: var(--muted); }
.who strong { color: var(--text); font-weight: 600; }
.who:hover strong { text-decoration: underline; }
.progress { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--muted); font-size: 12.5px; }
.composer {
  position: relative; flex: none; margin: 0 16px 16px; padding: 10px 10px 8px; border-radius: 18px;
  background: var(--panel); border: 1px solid var(--border); box-shadow: 0 8px 30px rgba(0, 0, 0, 0.12);
  display: flex; flex-direction: column; gap: 6px;
}
.composer:focus-within { border-color: color-mix(in srgb, var(--accent) 60%, var(--border)); }
.composer textarea { width: 100%; border: 0; background: transparent; padding: 6px 6px 2px; font: inherit; font-size: var(--agent-size); line-height: 1.45; max-height: 200px; overflow-y: auto; resize: none; color: var(--text); outline: none; user-select: text; }
.composer textarea::placeholder { color: var(--faint); }
.row { display: flex; align-items: center; gap: 6px; min-width: 0; }
.row .spacer { flex: 1 1 auto; }
.hint { padding-left: 6px; font-size: 11.5px; color: var(--muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mentions { position: absolute; left: 10px; bottom: calc(100% + 6px); z-index: 20; min-width: 240px; max-width: 360px; max-height: 240px; overflow-y: auto; margin: 0; padding: 4px; list-style: none; border-radius: 12px; background: var(--panel); border: 1px solid var(--border); box-shadow: 0 8px 30px rgba(0, 0, 0, 0.35); }
.mentions li { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 8px; font-size: 13px; color: var(--text); cursor: pointer; min-width: 0; }
.mentions li span { font-size: 11.5px; color: var(--faint); }
.mentions li.on { background: var(--hover); }
.icon { display: inline-grid; place-items: center; padding: 0; border: 1px solid transparent; background: transparent; }
.send { width: 34px; height: 34px; border-radius: 50%; flex: none; }
.send.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
.send.primary:hover:not(:disabled) { background: #9459f0; border-color: #9459f0; }
</style>
