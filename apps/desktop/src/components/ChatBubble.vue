<script setup lang="ts">
// Uma mensagem da conversa, no desenho do ovrgit (AgentWindow.vue): a sua em balao a direita (hora e copiar ao
// passar o mouse), a do DOT como texto corrido em markdown, com o grupo de ferramentas e o cartao de erro.
import { computed, ref, watch } from "vue";
import type { Message } from "@dots/shared-types";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Copy, FileText, ImageIcon, Mic } from "lucide-vue-next";
import ToolGroup from "@/components/ToolGroup.vue";
import { mediaApi } from "@/api";
import { describeFailure } from "@/lib/humanize";
import { md, onMdClick } from "@/lib/markdown";

const props = defineProps<{ message: Message; last?: boolean; busy?: boolean; mentions?: string[] }>();
const emit = defineEmits<{ resend: []; zoom: [img: { src: string; name: string }] }>();
const meta = computed(() => (props.message.metadata ?? {}) as { tools?: string[]; error?: string | null; streaming?: boolean; origin?: string; prompt?: string; status?: string; attachments?: string[] });
const tools = computed(() => (meta.value.tools ?? []).map((name) => ({ name })));
const time = computed(() => new Date(props.message.created_at).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" }));

/** Texto da sua mensagem em pedacos, com os @Nome marcados destacados (grupo geral). */
const parts = computed(() => {
  const names = [...(props.mentions ?? [])].sort((a, b) => b.length - a.length);
  if (!names.length) return [{ text: props.message.content, mention: false }];
  const esc = names.map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  return props.message.content.split(new RegExp(`(@(?:${esc.join("|")}))`, "g")).filter(Boolean).map((text) => ({ text, mention: names.some((n) => text === `@${n}`) }));
});

/** Mensagens longas aparecem recolhidas no balao; clique mostra tudo */
const LONG_USER = 700;
const LONG_LINES = 12;
const long = computed(() => props.message.content.length > LONG_USER || props.message.content.split("\n").length > LONG_LINES);
const expanded = ref(false);
const copied = ref(false);
async function copy() {
  try {
    await navigator.clipboard.writeText(props.message.content);
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  } catch {
    /* sem acesso a area de transferencia */
  }
}

// anexos da mensagem: imagens viram miniatura (clique amplia), o resto um chip com o nome
const fileName = (p: string) => p.split("/").pop() ?? p;
const kindOf = (p: string) => (/\.(png|jpe?g|gif|webp)$/i.test(p) ? "image" : /\.(m4a|mp3|wav|aac|ogg|webm)$/i.test(p) ? "audio" : "file");
const previews = ref<Record<string, string>>({});
watch(
  () => meta.value.attachments,
  async (list) => {
    for (const p of list ?? []) {
      if (kindOf(p) !== "image" || previews.value[p]) continue;
      const url = await mediaApi.imagePreview(p);
      if (url) previews.value = { ...previews.value, [p]: url };
    }
  },
  { immediate: true },
);

const errorInfo = computed(() => {
  const e = meta.value.error;
  if (!e) return null;
  const kind = meta.value.status === "TIMEOUT" ? "timeout" : meta.value.status === "CANCELLED" ? "cancelled" : "engine";
  return { title: kind === "cancelled" ? "Cancelado" : kind === "timeout" ? "Passou do tempo" : "Não deu certo", text: describeFailure(kind, e), detail: kind === "engine" ? null : e };
});
/** Quando o motor falha, o texto da resposta e o proprio erro: o cartao ja mostra */
const showText = computed(() => !!props.message.content.trim() && props.message.content.trim() !== meta.value.error?.trim());
</script>

<template>
  <div v-if="message.role === 'system'" class="note-row">
    <details class="note">
      <summary>{{ message.content }}</summary>
      <p v-if="meta.prompt">{{ meta.prompt }}</p>
    </details>
  </div>

  <div v-else-if="message.role === 'user'" class="user">
    <div class="bubble">
      <div v-if="meta.attachments?.length" class="atts">
        <template v-for="a in meta.attachments" :key="a">
          <button v-if="previews[a]" type="button" class="thumb" :title="`${fileName(a)}: clique para ampliar`" @click="emit('zoom', { src: previews[a]!, name: fileName(a) })">
            <img :src="previews[a]" :alt="fileName(a)" />
          </button>
          <span v-else class="att" :title="a">
            <ImageIcon v-if="kindOf(a) === 'image'" :size="12" />
            <Mic v-else-if="kindOf(a) === 'audio'" :size="12" />
            <FileText v-else :size="12" />
            <span class="ellipsis">{{ fileName(a) }}</span>
          </span>
        </template>
      </div>
      <p v-if="message.content" :class="{ clamp: long && !expanded }"><template v-for="(pt, i) in parts" :key="i"><span v-if="pt.mention" class="mention">{{ pt.text }}</span><template v-else>{{ pt.text }}</template></template></p>
      <button v-if="long" type="button" class="more" @click="expanded = !expanded">{{ expanded ? "Mostrar menos" : "Mostrar tudo" }}</button>
    </div>
    <div class="user-acts" :class="{ on: copied }">
      <small>{{ time }}</small>
      <button type="button" class="ua" :title="copied ? 'Copiado' : 'Copiar mensagem'" @click="copy">
        <Check v-if="copied" :size="12" />
        <Copy v-else :size="12" />
      </button>
    </div>
  </div>

  <div v-else class="answer">
    <div v-if="showText" class="md" @click="onMdClick($event, openUrl)" v-html="md(message.content || (meta.streaming ? '…' : ''), !meta.streaming)" />
    <ToolGroup v-if="tools.length" :items="tools" />
    <div v-if="errorInfo" class="stopped">
      <strong>{{ errorInfo.title }}</strong>
      <p>{{ errorInfo.text }}</p>
      <small v-if="errorInfo.detail" class="stop-detail">{{ errorInfo.detail }}</small>
      <div v-if="last && !busy" class="stop-actions">
        <button type="button" class="small" @click="emit('resend')">Reenviar mensagem</button>
      </div>
    </div>
    <p class="meta">{{ time }}</p>
  </div>
</template>

<style scoped>
.note-row { display: flex; justify-content: center; }
.note { max-width: 80%; padding: 6px 12px; border-radius: 12px; border: 1px dashed var(--border); color: var(--muted); font-size: 12.5px; text-align: center; }
.note summary { cursor: pointer; list-style: none; }
.note p { margin: 6px 0 0; white-space: pre-wrap; text-align: left; color: var(--faint); user-select: text; }

.user { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }
/* hora e copiar: aparecem ao passar o mouse na mensagem */
.user-acts { display: flex; align-items: center; gap: 4px; height: 22px; opacity: 0; transition: opacity 0.12s; color: var(--faint); }
.user:hover .user-acts, .user-acts.on, .user-acts:focus-within { opacity: 1; }
.user-acts small { font-size: 11px; font-variant-numeric: tabular-nums; }
.ua { display: grid; place-items: center; width: 22px; height: 22px; padding: 0; color: var(--faint); border-radius: 6px; }
.ua:hover { color: var(--text); background: var(--hover); }
.user-acts.on .ua { color: var(--add); }
.bubble { max-width: 78%; padding: 10px 14px; border-radius: 16px 16px 4px 16px; background: var(--panel-2); border: 1px solid var(--border); display: flex; flex-direction: column; gap: 8px; }
.bubble .mention { color: var(--accent); font-weight: 600; }
.bubble p { margin: 0; font-size: var(--agent-size); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; line-height: 1.5; }
.bubble p.clamp { display: -webkit-box; -webkit-line-clamp: 8; -webkit-box-orient: vertical; overflow: hidden; }
.more { height: 22px; padding: 0 6px; margin: -4px 0 0 -6px; align-self: flex-start; font-size: 11.5px; color: var(--accent); border-radius: 6px; }
.more:hover { background: var(--hover); }
.atts { display: flex; flex-wrap: wrap; gap: 6px; }
.att { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 8px 0 6px; max-width: 220px; border-radius: 8px; background: var(--panel); border: 1px solid var(--border); font-size: 12px; color: var(--muted); }
.thumb { position: relative; display: block; width: 120px; height: 90px; padding: 0; flex: none; border-radius: 12px; overflow: hidden; border: 1px solid var(--border); background: var(--panel-2); cursor: zoom-in; }
.thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
.thumb:hover { border-color: var(--accent); }

.answer { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.meta { margin: 0; font-size: 11px; color: var(--faint); font-variant-numeric: tabular-nums; }
.stopped { display: flex; flex-direction: column; gap: 6px; max-width: 640px; padding: 12px 14px; border-radius: 12px; border: 1px solid color-mix(in srgb, var(--del) 30%, var(--border)); background: color-mix(in srgb, var(--del) 6%, var(--panel)); }
.stopped strong { font-size: 13.5px; color: var(--del); }
.stopped p { margin: 0; font-size: 13px; color: var(--muted); line-height: 1.5; user-select: text; overflow-wrap: anywhere; }
.stop-detail { font-family: var(--mono); font-size: 11.5px; color: var(--faint); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
.stop-actions { display: flex; gap: 8px; margin-top: 4px; }
.stop-actions .small { height: 28px; padding: 0 10px; font-size: 12px; font-weight: 500; border-radius: 8px; border: 1px solid var(--border); background: var(--panel); }
.stop-actions .small:hover { background: var(--hover); }
</style>
