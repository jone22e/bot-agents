<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { Message } from "@dots/shared-types";
import { ArrowUp, Check, FileText, ImageIcon, List, Mic, Monitor, Paperclip, Square, X } from "lucide-vue-next";
import Avatar from "@/components/Avatar.vue";
import ChatBubble from "@/components/ChatBubble.vue";
import ComputerPanel from "@/components/ComputerPanel.vue";
import ProfilePanel from "@/components/ProfilePanel.vue";
import RequestCard from "@/components/RequestCard.vue";
import ToolGroup from "@/components/ToolGroup.vue";
import { api, mediaApi, RuntimeError } from "@/api";
import { useDictation } from "@/composables/useDictation";
import { toolName } from "@/lib/humanize";
import { md, onMdClick } from "@/lib/markdown";
import { useApprovalsStore } from "@/stores/approvals";
import { useChatStore } from "@/stores/chat";
import { useContactsStore } from "@/stores/contacts";
import { useDotsStore } from "@/stores/dots";
import { useRuntimeStore } from "@/stores/runtime";

const route = useRoute();
const router = useRouter();
const chat = useChatStore();
const contacts = useContactsStore();
const dots = useDotsStore();
const approvals = useApprovalsStore();
const runtime = useRuntimeStore();
const draft = ref("");
const showProfile = ref(false);
const showComputer = ref(false);
const thread = ref<HTMLElement | null>(null);
const box = ref<HTMLTextAreaElement | null>(null);

const dotId = computed(() => (route.params.dotId as string | undefined) ?? null);
const dot = computed(() => dots.items.find((d) => d.id === dotId.value) ?? contacts.byDot(dotId.value ?? "")?.dot ?? null);
const pendingApprovals = computed(() => chat.pendingFor(approvals.pending));

type Row = { kind: "date"; key: string; label: string } | { kind: "msg"; key: string; m: Message; last: boolean };
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  let last = "";
  chat.messages.forEach((m, i) => {
    const d = new Date(m.created_at);
    const day = d.toDateString();
    if (day !== last) {
      last = day;
      const today = new Date();
      const yesterday = new Date(Date.now() - 86400000);
      const label = day === today.toDateString() ? "Hoje" : day === yesterday.toDateString() ? "Ontem" : d.toLocaleDateString("pt-BR", { day: "2-digit", month: "short" });
      out.push({ kind: "date", key: `d-${day}`, label });
    }
    out.push({ kind: "msg", key: m.id, m, last: i === chat.messages.length - 1 });
  });
  return out;
});

async function load() {
  if (!runtime.isConnected) return;
  if (!dotId.value) {
    if (contacts.items.length === 0) await contacts.load();
    if (contacts.items[0]) router.replace(`/chat/${contacts.items[0].dot.id}`);
    return;
  }
  if (dots.items.length === 0) await dots.load();
  await chat.open(dotId.value);
}
onMounted(load);
watch(dotId, () => {
  showProfile.value = false;
  showComputer.value = false;
  clearComposer();
  load();
});
watch(() => runtime.isConnected, (v) => v && load());

// ---------- andamento: tempo desde que o DOT comecou e o que esta fazendo ----------
const now = ref(Date.now());
const clock = setInterval(() => (now.value = Date.now()), 1000);
onUnmounted(() => clearInterval(clock));
const elapsed = computed(() => {
  const s = Math.max(0, Math.round((now.value - (chat.live.since ?? now.value)) / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
});
const activity = computed(() => {
  const l = chat.live;
  if (l.status === "queued") return "Esperando a vez…";
  if (l.status === "waiting_approval") return "Esperando sua resposta…";
  const running = l.tools.find((t) => t.ok === null);
  if (running) return `${toolName(running.name)}…`;
  return l.text ? "Escrevendo…" : "Pensando…";
});
/** Conversa concluida: a ultima mensagem e uma resposta do DOT sem erro e nada esta rodando */
const done = computed(() => {
  const m = chat.messages[chat.messages.length - 1];
  return !chat.busy && !pendingApprovals.value.length && m?.role === "assistant" && !(m.metadata as { error?: string } | null)?.error;
});

// ---------- rolagem: acompanha o fim enquanto o usuario nao subiu para ler algo ----------
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
watch(() => [chat.messages.length, chat.live.text, chat.live.tools.length, pendingApprovals.value.length], () => scrollToEnd());
watch(() => chat.conversationId, () => scrollToEnd(true));

// ---------- anexos: clipe, arrastar, colar ----------
interface Pending { path: string; name: string; size: number; preview?: string }
const pending = reactive<Pending[]>([]);
const attachError = ref<string | null>(null);
const dragging = ref(false);
const isImage = (name: string) => /\.(png|jpe?g|gif|webp)$/i.test(name);
const kindOf = (name: string) => (isImage(name) ? "image" : /\.(m4a|mp3|wav|aac|ogg|webm)$/i.test(name) ? "audio" : "file");
const sizeOf = (n: number) => (!n ? "" : n < 1024 * 1024 ? `${Math.max(1, Math.round(n / 1024))} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);
const errText = (e: unknown) => (e instanceof RuntimeError ? e.message : String((e as Error)?.message ?? e));

function pushPending(a: Pending) {
  if (pending.some((p) => p.path === a.path)) return;
  if (pending.length >= 20) {
    attachError.value = "Máximo de 20 anexos por mensagem.";
    return;
  }
  pending.push(a);
}
/** Arquivo do computador: o runtime guarda onde o DOT pode ler (ou mantem, se ja esta numa pasta dele) */
async function attachPath(path: string) {
  if (!chat.conversationId) return;
  try {
    const name = path.split("/").pop() ?? path;
    const saved = await api.conversations.attach(chat.conversationId, name, { path });
    pushPending({ ...saved, preview: isImage(saved.name) ? (await mediaApi.imagePreview(saved.path)) ?? undefined : undefined });
  } catch (e) {
    attachError.value = errText(e);
  }
}
async function pickFiles() {
  attachError.value = null;
  const f = await open({ multiple: true, title: "Anexar arquivos" });
  for (const p of Array.isArray(f) ? f : typeof f === "string" ? [f] : []) await attachPath(p);
}
const readAs = (f: File, how: "url" | "b64") =>
  new Promise<string>((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => {
      const url = String(r.result);
      resolve(how === "url" ? url : url.slice(url.indexOf(",") + 1));
    };
    r.onerror = () => reject(r.error);
    r.readAsDataURL(f);
  });
/** Arquivos colados (so bytes, sem caminho): o runtime guarda em `anexos/` da pasta do DOT */
async function addBlobs(files: File[]) {
  attachError.value = null;
  if (!chat.conversationId) return;
  for (const f of files) {
    try {
      const name = f.name && f.name !== "image.png" ? f.name : `colado-${new Date().toISOString().slice(11, 19).replace(/:/g, "")}.${(f.type.split("/")[1] || "png").replace("jpeg", "jpg")}`;
      const saved = await api.conversations.attach(chat.conversationId, name, { data: await readAs(f, "b64") });
      pushPending({ ...saved, preview: f.type.startsWith("image/") ? await readAs(f, "url") : undefined });
    } catch (e) {
      attachError.value = errText(e);
    }
  }
}
function removePending(i: number) {
  pending.splice(i, 1);
}

// arrastar arquivos do Finder: o Tauri entrega os caminhos (o evento HTML nao traz os arquivos no webview)
let unlistenDrop: (() => void) | null = null;
onMounted(async () => {
  unlistenDrop = await getCurrentWebview().onDragDropEvent((e) => {
    const p = e.payload;
    if (p.type === "enter") dragging.value = !!dot.value && p.paths.length > 0;
    else if (p.type === "leave") dragging.value = false;
    else if (p.type === "drop") {
      dragging.value = false;
      if (dot.value) p.paths.forEach((x) => attachPath(x));
    }
  });
});
onUnmounted(() => unlistenDrop?.());

/** Texto colado grande vira um cartao (como no app do ChatGPT), em vez de tomar o campo inteiro */
const PASTE_CHARS = 1200;
const PASTE_LINES = 12;
const pastes = reactive<{ id: string; text: string; lines: number }[]>([]);
function onPaste(e: ClipboardEvent) {
  const files = [...(e.clipboardData?.items ?? [])].filter((i) => i.kind === "file").map((i) => i.getAsFile()).filter((f): f is File => !!f);
  if (files.length) {
    e.preventDefault();
    addBlobs(files);
    return;
  }
  const text = e.clipboardData?.getData("text/plain") ?? "";
  const lines = text.split("\n").length;
  if (text.length > PASTE_CHARS || lines > PASTE_LINES) {
    e.preventDefault();
    pastes.push({ id: crypto.randomUUID(), text, lines });
  }
}
function pasteToField(i: number) {
  const [p] = pastes.splice(i, 1);
  if (!p) return;
  draft.value = draft.value ? `${draft.value}\n${p.text}` : p.text;
  nextTick(autosize);
}
const pastePreview = (t: string) => t.trim().split("\n")[0]!.slice(0, 60);

/** Imagem aberta em tamanho maior (clique na miniatura) */
const zoom = ref<{ src: string; name: string } | null>(null);
const onZoomKey = (e: KeyboardEvent) => e.key === "Escape" && (zoom.value = null);
watch(zoom, (z) => (z ? window.addEventListener("keydown", onZoomKey) : window.removeEventListener("keydown", onZoomKey)));
onUnmounted(() => window.removeEventListener("keydown", onZoomKey));

// ---------- ditado: um clique grava, outro transcreve para o campo ----------
const dictation = useDictation((text) => {
  draft.value = draft.value.trim() ? `${draft.value.replace(/\s+$/, "")} ${text}` : text;
  nextTick(() => {
    autosize();
    box.value?.focus();
  });
});

// ---------- envio ----------
const canCompose = computed(() => !!(draft.value.trim() || pastes.length || pending.length));
const canStart = computed(() => !!dot.value && chat.dotId === dot.value.id && !!chat.conversationId && !chat.messages.length && !chat.busy && !chat.sending && !canCompose.value);
const START_MESSAGE = "Comece a trabalhar no objetivo que defini para você. Se faltar alguma informação para iniciar, me pergunte.";
function clearComposer() {
  draft.value = "";
  pastes.splice(0);
  pending.splice(0);
  attachError.value = null;
  nextTick(autosize);
}
async function send() {
  if (!canCompose.value || chat.sending) return;
  const body = [draft.value.trim(), ...pastes.map((p) => p.text)].filter(Boolean).join("\n\n");
  const att = pending.map((p) => p.path);
  clearComposer();
  scrollToEnd(true);
  await chat.send(body, att);
  await contacts.load();
}
async function startWork() {
  if (!canStart.value) return;
  await chat.send(START_MESSAGE);
  await contacts.load();
  scrollToEnd(true);
}
/** "Reenviar mensagem" no cartao de erro: a ultima mensagem sua, com os mesmos anexos */
async function resend() {
  const m = [...chat.messages].reverse().find((x) => x.role === "user");
  if (!m) return;
  await chat.send(m.content, ((m.metadata ?? {}) as { attachments?: string[] }).attachments ?? []);
}
function onKey(e: KeyboardEvent) {
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
  <div class="flex h-full min-w-0">
    <div class="chat flex min-w-0 flex-1 flex-col">
      <div v-if="!dot" class="flex flex-1 items-center justify-center text-sm text-zinc-500">
        <div class="text-center">
          <p>{{ runtime.isConnected ? "Escolha um DOT a esquerda ou crie um novo." : "Ligando o servico..." }}</p>
          <router-link v-if="runtime.isConnected" to="/dots/new" class="btn-primary mt-3">Criar DOT</router-link>
        </div>
      </div>
      <template v-else>
        <header class="relative flex items-center justify-center py-3" data-tauri-drag-region>
          <button class="flex items-center gap-2 rounded-full bg-zinc-800/90 px-3 py-1.5 text-sm font-medium text-zinc-100 hover:bg-zinc-700" @click="showProfile = !showProfile; if (showProfile) showComputer = false">
            <Avatar :name="dot.name" size="sm" :state="contacts.byDot(dot.id)?.state" />
            {{ dot.name }}
          </button>
          <button class="absolute right-4 flex h-9 w-9 items-center justify-center rounded-full text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100" :class="showComputer ? 'bg-zinc-800 text-zinc-100' : ''" :title="`Computador de ${dot.name}`" @click="showComputer = !showComputer; if (showComputer) showProfile = false"><Monitor :size="18" /></button>
        </header>
        <p v-if="chat.error" class="mx-auto mb-2 rounded-md bg-red-950/40 px-3 py-1 text-xs text-red-200">{{ chat.error }}</p>

        <main ref="thread" class="thread" @scroll="onScroll">
          <div v-if="!rows.length && !chat.busy" class="empty">
            <Avatar :name="dot.name" size="lg" />
            <h2>Pronto para começar</h2>
            <p>{{ dot.name }} inicia automaticamente ao ser criado. Se ainda não começou, use o botão para pedir que siga o objetivo definido, ou escreva abaixo.</p>
            <button v-if="!canCompose" type="button" class="btn-primary mt-1" :disabled="!canStart" @click="startWork">{{ chat.sending ? "Iniciando..." : "Iniciar trabalho" }}</button>
          </div>
          <template v-for="r in rows" :key="r.key">
            <p v-if="r.kind === 'date'" class="day">{{ r.label }}</p>
            <ChatBubble v-else :message="r.m" :last="r.last" :busy="chat.busy" @resend="resend" @zoom="(z) => (zoom = z)" />
          </template>
          <RequestCard v-for="a in pendingApprovals" :key="a.id" :approval="a" :busy="approvals.deciding !== null" @decide="(ok, ans) => approvals.decide(a.id, ok, ans)" />
          <div v-if="chat.busy" class="answer">
            <div v-if="chat.live.text" class="md" @click="onMdClick($event, openUrl)" v-html="md(chat.live.text, false)" />
            <ToolGroup v-if="chat.live.tools.length" :items="chat.live.tools" />
            <div class="progress">
              <Avatar :name="dot.name" size="sm" class="spin-logo" />
              <span class="mono">{{ elapsed }}</span> · {{ activity }}
            </div>
          </div>
          <!-- conversa concluida: uma linha verde fecha a conversa; some quando o DOT volta a trabalhar -->
          <div v-if="done" class="done-mark" role="status">
            <span class="dm-line" /><span class="dm-label"><span class="dm-check"><Check :size="9" :stroke-width="3" /></span> Concluído</span><span class="dm-line" />
          </div>
        </main>

        <footer class="composer" :class="{ drop: dragging }">
          <div v-if="dragging" class="drop-hint"><Paperclip :size="16" /> Solte para anexar</div>
          <div v-if="pending.length" class="pending">
            <template v-for="(a, i) in pending" :key="a.path">
              <span v-if="a.preview" class="thumb" :title="`${a.name}${a.size ? ` · ${sizeOf(a.size)}` : ''}: clique para ampliar`" @click="zoom = { src: a.preview, name: a.name }">
                <img :src="a.preview" :alt="a.name" />
                <button type="button" class="thumb-rm" title="Remover" @click.stop="removePending(i)"><X :size="11" /></button>
              </span>
              <span v-else class="att" :title="`${a.path}${a.size ? ` · ${sizeOf(a.size)}` : ''}`">
                <ImageIcon v-if="kindOf(a.name) === 'image'" :size="12" />
                <Mic v-else-if="kindOf(a.name) === 'audio'" :size="12" />
                <FileText v-else :size="12" />
                <span class="ellipsis">{{ a.name }}</span>
                <button type="button" class="rm" title="Remover" @click="removePending(i)"><X :size="11" /></button>
              </span>
            </template>
          </div>
          <div v-if="pastes.length" class="pending">
            <span v-for="(p, i) in pastes" :key="p.id" class="att paste" :title="p.text.slice(0, 400)">
              <List :size="12" />
              <span class="paste-text">
                <span class="ellipsis">{{ pastePreview(p.text) }}</span>
                <small>Texto colado · {{ p.lines }} linhas · <a @click.prevent="pasteToField(i)">mostrar no campo</a></small>
              </span>
              <button type="button" class="rm" title="Remover" @click="pastes.splice(i, 1)"><X :size="11" /></button>
            </span>
          </div>
          <p v-if="attachError" class="att-err">{{ attachError }}</p>
          <p v-if="dictation.error.value" class="att-err">{{ dictation.error.value }}</p>
          <textarea
            ref="box"
            v-model="draft"
            rows="1"
            :placeholder="chat.busy ? `${dot.name} está trabalhando… Enter envia em seguida` : `Mensagem para ${dot.name}`"
            @input="autosize"
            @keydown="onKey"
            @paste="onPaste"
          />
          <div class="row">
            <button type="button" class="icon attach" title="Anexar arquivos (ou arraste/cole na janela)" @click="pickFiles"><Paperclip :size="15" /></button>
            <span class="spacer" />
            <div v-if="dictation.supported" class="dictate" :class="dictation.phase.value">
              <template v-if="dictation.phase.value === 'recording'">
                <button type="button" class="icon dict-x" title="Descartar a gravação" @click="dictation.cancel"><X :size="13" /></button>
                <span class="dict-time mono">{{ dictation.time.value }}</span>
              </template>
              <span v-if="dictation.phase.value === 'transcribing'" class="dict-time">Transcrevendo…</span>
              <button
                type="button"
                class="icon mic"
                :disabled="dictation.phase.value === 'transcribing'"
                :title="dictation.phase.value === 'recording' ? 'Terminar e transcrever' : dictation.phase.value === 'transcribing' ? 'Transcrevendo o áudio…' : 'Ditar: grava o microfone e escreve o texto no campo'"
                @click="dictation.toggle"
              >
                <span v-if="dictation.phase.value === 'transcribing'" class="spinner" />
                <Square v-else-if="dictation.phase.value === 'recording'" :size="13" fill="currentColor" />
                <Mic v-else :size="15" />
              </button>
            </div>
            <button v-if="chat.busy && !canCompose" type="button" class="icon send stop" title="Interromper" @click="chat.cancel()"><Square :size="13" fill="currentColor" /></button>
            <template v-else>
              <button v-if="chat.busy" type="button" class="icon mic" title="Interromper" @click="chat.cancel()"><Square :size="13" fill="currentColor" /></button>
              <button type="button" class="icon send primary" title="Enviar (Enter)" :disabled="!canCompose || chat.sending" @click="send"><ArrowUp :size="16" /></button>
            </template>
          </div>
        </footer>
      </template>
    </div>
    <ProfilePanel v-if="dot && showProfile" :dot="dot" @close="showProfile = false" @deleted="showProfile = false; router.replace('/')" />
    <ComputerPanel v-if="dot && showComputer" :dot="dot" @close="showComputer = false" />

    <!-- imagem anexada em tamanho maior: clique fora ou Esc fecha -->
    <div v-if="zoom" class="zoom" @click="zoom = null">
      <img :src="zoom.src" :alt="zoom.name" @click.stop />
      <button type="button" class="zoom-close" title="Fechar (Esc)" @click="zoom = null"><X :size="16" /></button>
    </div>
  </div>
</template>

<style scoped>
/* so rola na vertical: textos longos quebram e codigo/tabelas rolam por dentro do proprio bloco */
.thread { flex: 1; overflow-y: auto; overflow-x: hidden; padding: 20px 22px 12px; display: flex; flex-direction: column; gap: 22px; }
.empty { margin: auto; max-width: 520px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 14px; }
.empty h2 { margin: 0; font-size: 17px; font-weight: 600; letter-spacing: -0.01em; }
.empty p { margin: 0; font-size: 12.5px; line-height: 1.5; color: var(--faint); }
.day { margin: 0; text-align: center; font-size: 11px; color: var(--faint); }

.answer { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.progress { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12.5px; padding: 2px 0; }
.progress .mono { font-size: 12px; font-variant-numeric: tabular-nums; }
.progress :deep(.spin-logo) { width: 18px; height: 18px; font-size: 9px; margin-right: 2px; animation: breathe 1.6s ease-in-out infinite; }
@keyframes breathe { 50% { opacity: 0.35; transform: scale(0.9); } }

/* marca de conversa concluida, no fim da conversa */
.done-mark { flex: none; display: flex; align-items: center; gap: 12px; margin: -4px 0 2px; animation: dm-in 0.25s ease-out; }
.dm-line { flex: 1; height: 1px; background: linear-gradient(to right, transparent, var(--border)); }
.dm-line:last-child { background: linear-gradient(to left, transparent, var(--border)); }
.dm-label { display: inline-flex; align-items: center; gap: 7px; flex: none; height: 24px; padding: 0 11px 0 5px; border-radius: 999px; border: 1px solid color-mix(in srgb, var(--add) 22%, var(--border)); background: color-mix(in srgb, var(--add) 7%, var(--panel)); color: var(--text); font-size: 11.5px; font-weight: 600; }
.dm-check { display: grid; place-items: center; width: 15px; height: 15px; border-radius: 50%; background: var(--add); color: var(--bg); }
@keyframes dm-in { from { opacity: 0; } }

.composer {
  position: relative; flex: none; margin: 0 16px 16px; padding: 10px 10px 8px; border-radius: 18px;
  background: var(--panel); border: 1px solid var(--border); box-shadow: 0 8px 30px rgba(0, 0, 0, 0.12);
  display: flex; flex-direction: column; gap: 6px;
}
.composer:focus-within { border-color: color-mix(in srgb, var(--accent) 60%, var(--border)); }
.composer.drop { border-color: var(--accent); background: var(--accent-soft); }
.drop-hint { display: flex; align-items: center; justify-content: center; gap: 8px; padding: 6px; color: var(--accent); font-weight: 600; font-size: 12.5px; }
.composer textarea { width: 100%; border: 0; background: transparent; padding: 6px 6px 2px; font: inherit; font-size: var(--agent-size); line-height: 1.45; max-height: 200px; overflow-y: auto; resize: none; color: var(--text); outline: none; user-select: text; }
.composer textarea::placeholder { color: var(--faint); }
.row { display: flex; align-items: center; gap: 6px; min-width: 0; }
.row .spacer { flex: 1 1 auto; }
.icon { display: inline-grid; place-items: center; padding: 0; border: 1px solid transparent; background: transparent; transition: background 0.12s, border-color 0.12s, opacity 0.12s; }
.icon:hover:not(:disabled) { background: var(--hover); }
.attach { width: 30px; height: 30px; border-radius: 50%; color: var(--muted); flex: none; }
.send { width: 34px; height: 34px; border-radius: 50%; flex: none; }
.send.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
.send.primary:hover:not(:disabled) { background: #9459f0; border-color: #9459f0; }
.send.stop { background: var(--text); color: var(--bg); border-color: var(--text); }
.send.stop:hover { background: var(--muted); }
.dictate { display: inline-flex; align-items: center; gap: 6px; flex: none; }
.mic { width: 34px; height: 34px; border-radius: 50%; flex: none; color: var(--muted); border-color: var(--border); background: var(--panel); }
.dictate.recording .mic { background: var(--del); border-color: var(--del); color: #fff; animation: mic-pulse 1.4s ease-in-out infinite; }
@keyframes mic-pulse { 50% { box-shadow: 0 0 0 5px color-mix(in srgb, var(--del) 25%, transparent); } }
.dict-time { font-size: 12px; color: var(--muted); }
.dictate.recording .dict-time { color: var(--del); }
.dict-x { width: 24px; height: 24px; border-radius: 50%; color: var(--faint); }

.pending { display: flex; flex-wrap: wrap; gap: 6px; padding: 2px 4px 0; }
.att { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 8px 0 6px; max-width: 220px; border-radius: 8px; background: var(--panel); border: 1px solid var(--border); font-size: 12px; color: var(--muted); }
.att .rm { display: grid; place-items: center; width: 18px; height: 18px; padding: 0; color: var(--faint); flex: none; border-radius: 4px; }
.att .rm:hover { color: var(--text); background: var(--hover); }
.att.paste { height: auto; padding: 6px 8px; max-width: 320px; align-items: flex-start; }
.paste-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; font-family: var(--mono); font-size: 11.5px; color: var(--text); }
.paste-text small { font-family: ui-sans-serif, system-ui, -apple-system, sans-serif; font-size: 11px; color: var(--faint); }
.paste-text a { color: var(--accent); cursor: pointer; }
.thumb { position: relative; display: block; width: 72px; height: 72px; flex: none; border-radius: 12px; overflow: hidden; border: 1px solid var(--border); background: var(--panel-2); cursor: zoom-in; }
.thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
.thumb:hover { border-color: var(--accent); }
.thumb-rm { position: absolute; top: 4px; right: 4px; width: 20px; height: 20px; padding: 0; border-radius: 50%; border: 0; display: grid; place-items: center; background: rgba(0, 0, 0, 0.6); color: #fff; }
.thumb-rm:hover { background: rgba(0, 0, 0, 0.85); }
.att-err { margin: 0 6px; font-size: 12px; color: var(--del); }

.zoom { position: fixed; inset: 0; z-index: 80; display: grid; place-items: center; padding: 32px; background: rgba(0, 0, 0, 0.78); cursor: zoom-out; animation: zoom-in 0.12s ease-out; }
.zoom img { max-width: 100%; max-height: 100%; border-radius: 8px; box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5); cursor: default; object-fit: contain; min-height: 0; }
.zoom-close { position: absolute; top: 14px; right: 14px; width: 32px; height: 32px; padding: 0; border-radius: 50%; border: 0; display: grid; place-items: center; background: rgba(255, 255, 255, 0.14); color: #fff; cursor: pointer; }
.zoom-close:hover { background: rgba(255, 255, 255, 0.26); }
@keyframes zoom-in { from { opacity: 0; } }
</style>
