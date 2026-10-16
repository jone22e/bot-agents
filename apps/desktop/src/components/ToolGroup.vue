<script setup lang="ts">
// Grupo de ferramentas da resposta: uma linha apagada; enquanto roda mostra o que esta fazendo, depois so o resumo.
// O clique abre a lista. Copiado do ovrgit (AgentWindow.vue: .tools).
import { computed, ref } from "vue";
import { Bot, Check, ChevronRight, Pencil, Search, Terminal, TriangleAlert, X, Zap } from "lucide-vue-next";
import { toolName } from "@/lib/humanize";

/** `ok`: null = rodando; ausente = resultado nao registrado (conversas antigas guardam so os nomes) */
const props = defineProps<{ items: { name: string; ok?: boolean | null }[] }>();
const open = ref(false);

type Kind = "cmd" | "read" | "edit" | "other";
const kindOf = (name: string): Kind => {
  const n = toolName(name);
  if (/^(Bash|run_command|shell)$/i.test(n)) return "cmd";
  if (/Edit|Write|write_file|delete_file/i.test(n)) return "edit";
  if (/Read|Grep|Glob|Search|Fetch|list|get|read/i.test(n)) return "read";
  return "other";
};
const ICON = { cmd: Terminal, read: Search, edit: Pencil, other: Zap } as const;
const iconOf = (name: string) => (/Agent|Task|delegate/i.test(toolName(name)) ? Bot : ICON[kindOf(name)]);

/** "8 comandos · 3 leituras · 2 edicoes" */
const summary = computed(() => {
  const n = { cmd: 0, read: 0, edit: 0, other: 0 };
  for (const i of props.items) n[kindOf(i.name)]++;
  const parts: string[] = [];
  if (n.cmd) parts.push(`${n.cmd} ${n.cmd === 1 ? "comando" : "comandos"}`);
  if (n.read) parts.push(`${n.read} ${n.read === 1 ? "leitura" : "leituras"}`);
  if (n.edit) parts.push(`${n.edit} ${n.edit === 1 ? "edição" : "edições"}`);
  if (n.other) parts.push(`${n.other} ${n.other === 1 ? "outra ação" : "outras ações"}`);
  return parts.join(" · ");
});
const busy = computed(() => props.items.some((i) => i.ok === null));
const failed = computed(() => props.items.some((i) => i.ok === false));
const now = computed(() => {
  const cur = props.items.find((i) => i.ok === null) ?? props.items[props.items.length - 1];
  return cur ? toolName(cur.name) : "";
});
</script>

<template>
  <div class="tools" :class="{ open }">
    <button type="button" class="tools-line" @click="open = !open">
      <span v-if="busy" class="spinner tiny" />
      <TriangleAlert v-else-if="failed" :size="12" class="bad" />
      <ChevronRight v-else :size="12" class="caret" />
      <span class="tools-sum">{{ summary }}</span>
      <span v-if="busy" class="tools-now mono ellipsis">{{ now }}</span>
    </button>
    <div v-if="open" class="tools-list">
      <div v-for="(t, i) in items" :key="i" class="tool">
        <div class="tool-line">
          <component :is="iconOf(t.name)" :size="12" class="tool-ic" />
          <span class="tool-title mono ellipsis">{{ toolName(t.name) }}</span>
          <span class="tool-state">
            <span v-if="t.ok === null" class="spinner tiny" />
            <Check v-else-if="t.ok" :size="11" class="ok" />
            <X v-else-if="t.ok === false" :size="11" class="bad" />
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tools { display: flex; flex-direction: column; gap: 2px; }
.tools-line { display: inline-flex; align-items: center; justify-content: flex-start; gap: 8px; height: 26px; padding: 0 8px; margin-left: -8px; border-radius: 8px; font-size: 12px; font-weight: 500; color: var(--faint); min-width: 0; max-width: 100%; align-self: flex-start; }
.tools-line:hover { color: var(--muted); background: var(--hover); }
.tools-line .caret { transition: transform 0.12s; flex: none; }
.tools.open .tools-line .caret { transform: rotate(90deg); }
.tools-sum { flex: none; }
.tools-now { min-width: 0; font-size: 11.5px; color: var(--faint); font-weight: 400; }
.tools-list { display: flex; flex-direction: column; gap: 1px; margin: 2px 0 4px 6px; padding-left: 10px; border-left: 2px solid var(--border); }
.tool { border-radius: 8px; border: 1px solid transparent; }
.tool-line { display: flex; align-items: center; gap: 8px; height: 26px; padding: 0 8px; min-width: 0; font-size: 12px; }
.tool-ic { color: var(--faint); flex: none; }
.tool-title { color: var(--faint); min-width: 0; font-size: 11.5px; }
.tool-state { margin-left: auto; flex: none; display: grid; place-items: center; width: 16px; }
.ok { color: var(--add); }
.bad { color: var(--del); }
</style>
