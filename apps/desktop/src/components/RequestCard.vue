<script setup lang="ts">
// O que o DOT espera do usuario dentro da conversa, no desenho do AskCard do ovrgit: a pergunta em destaque,
// opcoes numeradas com detalhe e o campo "Nao, e diga ao <DOT> o que fazer diferente" (Responder / Pular).
import { computed, ref } from "vue";
import type { Approval } from "@dots/shared-types";
import { ClipboardList, Pencil } from "lucide-vue-next";
import { describeApproval, RISK_LABEL } from "@/lib/humanize";

const props = defineProps<{ approval: Approval; busy: boolean }>();
const emit = defineEmits<{ decide: [approved: boolean, answer?: string] }>();
const other = ref("");
const details = ref(false);
const agent = computed(() => props.approval.dot_name || "DOT");
/** Permissao: as duas escolhas; a aprovada vira permissao aprendida (a demissao sempre pede). */
const choices = computed(() => [
  { label: "Sim, permitir", detail: props.approval.tool === "dots_dismiss_employee" ? "A demissao sempre pede a sua permissao." : `${agent.value} aprende e nao pede de novo para este tipo de acao.`, approved: true },
  { label: "Nao", detail: `${agent.value} segue sem esta acao.`, approved: false },
]);

function submit() {
  const text = other.value.trim();
  if (!text) return;
  other.value = "";
  // Pergunta: a resposta escrita; permissao: nega e diz o que fazer diferente.
  emit("decide", props.approval.kind === "question", text);
}
</script>

<template>
  <div class="ask">
    <template v-if="approval.kind === 'question'">
      <div class="ask-head">
        <p class="ask-q">{{ approval.summary }}</p>
      </div>
      <div v-if="approval.options.length" class="ask-opts">
        <button v-for="(o, i) in approval.options" :key="o" type="button" class="ghost ask-opt" :disabled="busy" @click="emit('decide', true, o)">
          <span class="num">{{ i + 1 }}</span>
          <span class="opt-body"><span class="opt-label">{{ o }}</span></span>
        </button>
      </div>
    </template>
    <template v-else>
      <div class="ask-head">
        <p class="ask-q">Posso {{ describeApproval(approval).replace(/^quer /, "") }}? <span class="risk">{{ approval.tool === 'dots_browser_open' ? 'acesso à internet' : RISK_LABEL[approval.risk] }}</span></p>
        <button type="button" class="ghost small see" title="Ver o que exatamente ele quer fazer" @click="details = !details"><ClipboardList :size="12" /> {{ details ? "Ocultar detalhes" : "Ver detalhes" }}</button>
      </div>
      <pre v-if="details" class="ask-details">{{ [approval.resources.join("\n"), JSON.stringify(approval.input, null, 1)].filter(Boolean).join("\n") }}</pre>
      <div class="ask-opts">
        <button v-for="(c, i) in choices" :key="c.label" type="button" class="ghost ask-opt" :disabled="busy" @click="emit('decide', c.approved)">
          <span class="num">{{ i + 1 }}</span>
          <span class="opt-body">
            <span class="opt-label">{{ c.label }}</span>
            <span class="opt-detail">{{ c.detail }}</span>
          </span>
        </button>
      </div>
    </template>
    <form class="ask-other" @submit.prevent="submit">
      <Pencil :size="13" class="pen" />
      <input v-model="other" type="text" :placeholder="approval.kind === 'question' ? `Responder a ${agent} com suas palavras` : `Nao, e diga ao ${agent} o que fazer diferente`" maxlength="2000" />
      <button v-if="other.trim()" type="submit" class="small primary" :disabled="busy">Responder</button>
      <button v-else-if="approval.kind === 'question'" type="button" class="small skip" :disabled="busy" @click="emit('decide', false)">Pular</button>
    </form>
  </div>
</template>

<!-- Medidas do AskCard do ovrgit; botoes com a base do ovrgit (styles.css), que aqui nao e global -->
<style scoped>
.ask { display: flex; flex-direction: column; gap: 14px; padding: 14px 16px 12px; border-radius: 14px; border: 1px solid var(--border); background: var(--panel); max-width: 640px; color: var(--text); }
.ask button { display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: 32px; padding: 0 14px; border-radius: 8px; border: 1px solid var(--border); background: var(--panel); font-weight: 500; white-space: nowrap; transition: background 0.12s, border-color 0.12s, opacity 0.12s; }
.ask button:hover:not(:disabled) { background: var(--hover); }
.ask button.ghost { background: transparent; border-color: transparent; }
.ask button.ghost:hover:not(:disabled) { background: var(--hover); }
.ask button.small { height: 26px; padding: 0 10px; font-size: 12px; }
.ask button.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
.ask button.primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); }
.ask-head { display: flex; align-items: flex-start; gap: 8px; }
.ask-q { margin: 0; flex: 1; min-width: 0; font-size: calc(var(--agent-size, 14px) + 0.5px); font-weight: 600; line-height: 1.45; user-select: text; overflow-wrap: anywhere; }
.risk { margin-left: 4px; font-size: 12px; font-weight: 500; color: var(--muted); }
.ask .see { flex: none; color: var(--accent); }
.ask-details { margin: -6px 0 0; padding: 8px 10px; border-radius: 10px; background: var(--panel-2); border: 1px solid var(--border); font-family: var(--mono); font-size: 11.5px; color: var(--muted); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; max-height: 220px; overflow-y: auto; }
.ask-opts { display: flex; flex-direction: column; gap: 2px; }
.ask .ask-opt { height: auto; padding: 7px 8px; border-radius: 10px; justify-content: flex-start; align-items: flex-start; gap: 12px; text-align: left; white-space: normal; color: var(--text); }
.ask-opt .num { width: 20px; height: 20px; border-radius: 50%; flex: none; display: grid; place-items: center; font-size: 11px; font-weight: 600; color: var(--muted); background: var(--panel-2); border: 1px solid var(--border); margin-top: 1px; }
.ask-opt:hover .num { color: var(--text); border-color: var(--accent); }
.opt-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.opt-label { font-size: var(--agent-size, 14px); font-weight: 600; line-height: 1.45; }
.opt-detail { font-size: calc(var(--agent-size, 14px) - 1px); line-height: 1.45; color: var(--muted); font-weight: 400; }
.ask-other { display: flex; align-items: center; gap: 8px; height: 36px; padding: 0 6px 0 10px; border-radius: 999px; border: 1px solid var(--border); background: var(--panel-2); }
.ask-other:focus-within { border-color: color-mix(in srgb, var(--accent) 60%, var(--border)); }
.ask-other .pen { color: var(--faint); flex: none; }
.ask-other input { flex: 1; min-width: 0; height: 100%; border: 0; background: transparent; padding: 0; font-size: calc(var(--agent-size, 14px) - 0.5px); color: var(--text); outline: none; user-select: text; }
.ask-other input::placeholder { color: var(--faint); }
.ask-other .small { height: 26px; border-radius: 999px; flex: none; }
.ask-other .skip { background: var(--panel); }
</style>
