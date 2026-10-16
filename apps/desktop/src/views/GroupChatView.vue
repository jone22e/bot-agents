<script setup lang="ts">
// Grupo geral: uma conversa em ordem de envio, com cada resposta identificada pelo gerente.
// Digitar @ abre a lista de gerentes: os marcados respondem; os outros recebem so para ciencia.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import type { Dot, GroupMessage } from "@dots/shared-types";
import { ArrowUp, Users } from "lucide-vue-next";
import { openUrl } from "@tauri-apps/plugin-opener";
import Avatar from "@/components/Avatar.vue";
import { md, onMdClick } from "@/lib/markdown";
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

type Row = { kind: "date"; key: string; label: string } | { kind: "msg"; key: string; g: GroupMessage };
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  let last = "";
  for (const g of group.messages) {
    if ((g.metadata as { streaming?: boolean } | null)?.streaming) continue;
    const d = new Date(g.created_at);
    const day = d.toDateString();
    if (day !== last) {
      last = day;
      const label = day === new Date().toDateString() ? "Hoje" : day === new Date(Date.now() - 86400000).toDateString() ? "Ontem" : d.toLocaleDateString("pt-BR", { day: "2-digit", month: "short" });
      out.push({ kind: "date", key: `d-${day}`, label });
    }
    out.push({ kind: "msg", key: g.id, g });
  }
  return out;
});
const timeOf = (g: GroupMessage) => new Date(g.created_at).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
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
watch(() => [group.messages.length, rows.value.length], () => scrollToEnd());
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
    <header class="group-header" data-tauri-drag-region>
      <span class="group-icon"><Users :size="18" /></span>
      <div><strong>Grupo geral</strong><small>{{ group.members.length }} {{ group.members.length === 1 ? "gerente" : "gerentes" }}</small></div>
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
        <div v-else class="message-row" :class="r.g.role === 'user' ? 'mine' : 'theirs'">
          <Avatar v-if="r.g.role === 'dot'" :name="r.g.dot_name ?? '?'" size="sm" />
          <div class="message-body">
            <button v-if="r.g.role === 'dot'" class="sender" :title="`Abrir a conversa com ${r.g.dot_name}`" @click="r.g.dot_id && router.push(`/chat/${r.g.dot_id}`)">{{ r.g.dot_name }}</button>
            <div class="message-bubble">
              <div v-if="r.g.role === 'dot'" class="md" @click="onMdClick($event, openUrl)" v-html="md(r.g.content)" />
              <p v-else>{{ r.g.content }}</p>
            </div>
            <small>{{ timeOf(r.g) }}</small>
          </div>
        </div>
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

<!-- Estilos globais da conversa (.chat, .md...) vem do ChatView. -->
<style scoped>
.group-header { display: flex; align-items: center; gap: 10px; padding: 12px 22px; border-bottom: 1px solid var(--border); }
.group-icon { display: grid; place-items: center; width: 36px; height: 36px; border-radius: 12px; background: var(--panel-2); color: var(--text); }
.group-header strong, .group-header small { display: block; }
.group-header strong { font-size: 14px; color: var(--text); }
.group-header small { font-size: 11px; color: var(--muted); }
.thread { flex: 1; overflow-y: auto; overflow-x: hidden; padding: 20px 22px 12px; display: flex; flex-direction: column; gap: 12px; }
.empty { margin: auto; max-width: 520px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 14px; }
.empty h2 { margin: 0; font-size: 17px; font-weight: 600; letter-spacing: -0.01em; }
.empty p { margin: 0; font-size: 12.5px; line-height: 1.5; color: var(--faint); }
.day { margin: 8px 0; text-align: center; font-size: 11px; color: var(--faint); }
.message-row { display: flex; align-items: flex-start; gap: 8px; min-width: 0; }
.message-row.mine { justify-content: flex-end; }
.message-body { display: flex; flex-direction: column; align-items: flex-start; gap: 4px; max-width: min(84%, 760px); min-width: 0; }
.mine .message-body { align-items: flex-end; }
.sender { padding: 3px 4px 0; background: transparent; border: 0; color: var(--text); font-size: 12px; font-weight: 600; }
.sender:hover { text-decoration: underline; }
.message-bubble { max-width: 100%; padding: 12px 16px; border-radius: 14px 14px 14px 4px; background: var(--panel); border: 1px solid var(--border); color: var(--text); font-size: var(--agent-size); line-height: 1.5; overflow-wrap: anywhere; user-select: text; }
.mine .message-bubble { border-radius: 15px 15px 4px 15px; background: var(--panel-2); }
.message-bubble p { margin: 0; white-space: pre-wrap; }
.message-bubble :deep(.md > :first-child) { margin-top: 0; }
.message-bubble :deep(.md > :last-child) { margin-bottom: 0; }
.message-body small { padding: 0 4px; color: var(--faint); font-size: 11px; font-variant-numeric: tabular-nums; }
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
