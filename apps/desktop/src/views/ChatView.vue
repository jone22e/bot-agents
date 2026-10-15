<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { open } from "@tauri-apps/plugin-dialog";
import type { Message } from "@dots/shared-types";
import { ArrowUp, Monitor, Paperclip, Square } from "lucide-vue-next";
import Avatar from "@/components/Avatar.vue";
import ChatBubble from "@/components/ChatBubble.vue";
import ComputerPanel from "@/components/ComputerPanel.vue";
import ProfilePanel from "@/components/ProfilePanel.vue";
import RequestCard from "@/components/RequestCard.vue";
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
const attachments = ref<string[]>([]);
const showProfile = ref(false);
const showComputer = ref(false);
const scroller = ref<HTMLElement | null>(null);

const dotId = computed(() => (route.params.dotId as string | undefined) ?? null);
const dot = computed(() => dots.items.find((d) => d.id === dotId.value) ?? contacts.byDot(dotId.value ?? "")?.dot ?? null);
const pending = computed(() => chat.pendingFor(approvals.pending));

type Row = { kind: "date"; key: string; label: string } | { kind: "msg"; key: string; m: Message };
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  let last = "";
  for (const m of chat.messages) {
    const d = new Date(m.created_at);
    const day = d.toDateString();
    if (day !== last) {
      last = day;
      const today = new Date();
      const yesterday = new Date(Date.now() - 86400000);
      const label = day === today.toDateString() ? "Hoje" : day === yesterday.toDateString() ? "Ontem" : d.toLocaleDateString([], { day: "2-digit", month: "short" });
      out.push({ kind: "date", key: `d-${day}`, label: `${label} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` });
    }
    out.push({ kind: "msg", key: m.id, m });
  }
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
watch(dotId, () => { showProfile.value = false; showComputer.value = false; load(); });
watch(() => runtime.isConnected, (v) => v && load());
watch(() => [chat.messages.length, chat.live.text, pending.value.length], () => nextTick(() => scroller.value?.scrollTo({ top: scroller.value.scrollHeight })));

async function send() {
  const text = draft.value;
  if (!text.trim()) return;
  draft.value = "";
  const att = attachments.value;
  attachments.value = [];
  await chat.send(text, att);
  await contacts.load();
}
async function pickAttachment() {
  const f = await open({ multiple: true, title: "Anexar arquivo das pastas do DOT" });
  if (Array.isArray(f)) attachments.value.push(...f);
  else if (typeof f === "string") attachments.value.push(f);
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); }
}
</script>

<template>
  <div class="flex h-full min-w-0">
    <div class="flex min-w-0 flex-1 flex-col">
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

        <div ref="scroller" class="flex-1 space-y-2 overflow-y-auto px-6 pb-4">
          <template v-for="r in rows" :key="r.key">
            <p v-if="r.kind === 'date'" class="py-3 text-center text-[11px] text-zinc-500">{{ r.label }}</p>
            <ChatBubble v-else :message="r.m" />
          </template>
          <p v-if="!rows.length && !chat.busy" class="pt-10 text-center text-xs text-zinc-500">Diga o que voce precisa. {{ dot.name }} responde aqui e tambem avisa quando algo acontecer.</p>
          <RequestCard v-for="a in pending" :key="a.id" :approval="a" :busy="approvals.deciding !== null" @decide="(ok, ans) => approvals.decide(a.id, ok, ans)" />
          <div v-if="chat.busy && !(chat.live.status === 'waiting_approval' && pending.length)" class="flex justify-start">
            <div class="max-w-[72%] rounded-2xl bg-zinc-800/90 px-4 py-2.5 text-[15px] text-zinc-100">
              <pre v-if="chat.live.text" class="whitespace-pre-wrap break-words font-sans">{{ chat.live.text }}</pre>
              <p v-else class="text-zinc-500">{{ chat.live.status === 'queued' ? 'Esperando a vez…' : chat.live.status === 'waiting_approval' ? 'Esperando sua resposta…' : 'Trabalhando…' }}</p>
              <div v-if="chat.live.tools.length" class="mt-1 flex flex-wrap gap-1 text-[11px]">
                <span v-for="(t, i) in chat.live.tools" :key="i" :class="t.ok === null ? 'text-sky-300 animate-pulse' : t.ok ? 'text-zinc-400' : 'text-red-300'">· {{ t.name }}</span>
              </div>
            </div>
          </div>
        </div>

        <div class="px-6 pb-4 pt-2">
          <div v-if="attachments.length" class="mb-2 flex flex-wrap gap-1">
            <span v-for="(a, i) in attachments" :key="a" class="badge bg-zinc-800 text-zinc-300">{{ a.split('/').pop() }} <button class="ml-1 text-zinc-500" @click="attachments.splice(i, 1)">×</button></span>
          </div>
          <div class="flex items-end gap-2 rounded-3xl border border-zinc-800 bg-zinc-900 px-2 py-1.5">
            <button class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-zinc-800 text-zinc-300 hover:bg-zinc-700" title="Anexar arquivo" @click="pickAttachment"><Paperclip :size="16" /></button>
            <textarea v-model="draft" rows="1" class="max-h-40 flex-1 resize-none bg-transparent px-1 py-2 text-[15px] text-zinc-100 placeholder-zinc-500 focus:outline-none" :placeholder="`Mensagem para ${dot.name}`" @keydown="onKey"></textarea>
            <button v-if="chat.busy" class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-zinc-800 text-red-300 hover:bg-zinc-700" title="Parar" @click="chat.cancel()"><Square :size="14" /></button>
            <button class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-zinc-100 text-zinc-900 hover:bg-white disabled:opacity-40" :disabled="chat.sending || !draft.trim()" title="Enviar" @click="send"><ArrowUp :size="18" /></button>
          </div>
        </div>
      </template>
    </div>
    <ProfilePanel v-if="dot && showProfile" :dot="dot" @close="showProfile = false" @deleted="showProfile = false; router.replace('/')" />
    <ComputerPanel v-if="dot && showComputer" :dot="dot" @close="showComputer = false" />
  </div>
</template>
