<script setup lang="ts">
import { computed, ref } from "vue";
import { Brain, ChevronLeft, ChevronRight, Plug, Shield, UserCircle, Users, Zap } from "lucide-vue-next";
import type { Dot } from "@dots/shared-types";
import Avatar from "@/components/Avatar.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import DotForm from "@/components/DotForm.vue";
import IntegrationPanel from "@/components/IntegrationPanel.vue";
import MemoryPanel from "@/components/MemoryPanel.vue";
import PolicyPanel from "@/components/PolicyPanel.vue";
import ResizeHandle from "@/components/ResizeHandle.vue";
import { useColumnWidth } from "@/composables/useColumnWidth";
import TeamPanel from "@/components/TeamPanel.vue";
import TriggersPanel from "@/components/TriggersPanel.vue";
import { ENGINE_LABEL } from "@/lib/humanize";
import { useContactsStore } from "@/stores/contacts";
import { useDotsStore } from "@/stores/dots";

const props = defineProps<{ dot: Dot }>();
const emit = defineEmits<{ close: []; deleted: [] }>();
const dots = useDotsStore();
const contacts = useContactsStore();
/** Largura do perfil, ajustavel pela alca na borda esquerda. */
const column = useColumnWidth("profile", 420, 320, 720);
/** Cargo, assistente e estado em uma linha: "Gerente de Google Ads · Codex (OpenAI) · supervisor". */
const subtitle = computed(() => [
  contacts.byDot(props.dot.id)?.title,
  props.dot.primary_engine ? ENGINE_LABEL[props.dot.primary_engine] : "sem assistente",
  props.dot.role === "supervisor" ? "supervisor" : null,
  props.dot.status === "paused" ? "pausado" : null,
].filter(Boolean).join(" · "));
type Section = "sobre" | "permissoes" | "gatilhos" | "equipe" | "memoria" | "aplicativos";
/** null = menu de secoes. */
const section = ref<Section | null>(null);
const saving = ref(false);
const confirmDelete = ref(false);
const sections: { id: Section; title: string; hint: string; icon: typeof Brain }[] = [
  { id: "sobre", title: "Sobre", hint: "Nome, o que ele faz e qual assistente usa", icon: UserCircle },
  { id: "permissoes", title: "Permissoes", hint: "O que ele pode fazer sozinho e em quais pastas", icon: Shield },
  { id: "gatilhos", title: "Gatilhos", hint: "Trabalhos automaticos por evento ou horario", icon: Zap },
  { id: "equipe", title: "Equipe", hint: "Quem ele gerencia e para quem trabalha", icon: Users },
  { id: "memoria", title: "Memoria", hint: "O que ele aprendeu e o que voce ensinou", icon: Brain },
  { id: "aplicativos", title: "Aplicativos", hint: "Conexao com o Flexi", icon: Plug },
];
const title = (s: Section) => sections.find((x) => x.id === s)!.title;

async function onUpdate(patch: Parameters<typeof dots.update>[1]) {
  saving.value = true;
  try { await dots.update(props.dot.id, patch); section.value = null; } finally { saving.value = false; }
}
async function remove() {
  confirmDelete.value = false;
  await dots.remove(props.dot.id);
  emit("deleted");
}
</script>

<template>
  <aside class="relative flex shrink-0 flex-col overflow-hidden border-l border-zinc-800/80 bg-zinc-950" :style="{ width: `${column.width.value}px` }">
    <ResizeHandle grows="left" overlay @resize="column.resize" @reset="column.reset" />
    <div class="flex items-center gap-3 border-b border-zinc-800/80 px-4 py-3">
      <template v-if="section">
        <button class="btn-ghost -ml-2 px-2" title="Voltar" @click="section = null"><ChevronLeft :size="18" /></button>
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm font-semibold text-zinc-100">{{ title(section) }}</p>
          <p class="truncate text-xs text-zinc-500">{{ dot.name }}</p>
        </div>
      </template>
      <template v-else>
        <Avatar :name="dot.name" size="md" />
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm font-semibold text-zinc-100">{{ dot.name }}</p>
          <p class="truncate text-xs text-zinc-500">{{ subtitle }}</p>
        </div>
      </template>
      <button class="btn-ghost px-2" @click="emit('close')">✕</button>
    </div>

    <div class="min-w-0 flex-1 overflow-y-auto overflow-x-hidden p-4">
      <template v-if="!section">
        <ul class="divide-y divide-zinc-800 overflow-hidden rounded-xl border border-zinc-800">
          <li v-for="s in sections" :key="s.id">
            <button class="flex w-full items-center gap-3 px-3 py-3 text-left hover:bg-zinc-900" @click="section = s.id">
              <component :is="s.icon" :size="18" class="shrink-0 text-zinc-400" />
              <span class="min-w-0 flex-1">
                <span class="block text-sm text-zinc-100">{{ s.title }}</span>
                <span class="block truncate text-xs text-zinc-500">{{ s.hint }}</span>
              </span>
              <ChevronRight :size="16" class="shrink-0 text-zinc-600" />
            </button>
          </li>
        </ul>
        <div class="mt-5 flex flex-wrap gap-2 text-xs">
          <button v-if="dot.status === 'active'" class="btn-secondary" @click="dots.pause(dot.id)">Pausar</button>
          <button v-else class="btn-secondary" @click="dots.resume(dot.id)">Retomar</button>
          <button class="btn-secondary" @click="dots.duplicate(dot.id)">Duplicar</button>
          <button class="btn-secondary" @click="dots.archive(dot.id)">Arquivar</button>
          <button class="btn-danger ml-auto" @click="confirmDelete = true">Excluir...</button>
        </div>
      </template>
      <DotForm v-else-if="section === 'sobre'" :dot="dot" :saving="saving" @update="onUpdate" @cancel="section = null" />
      <PolicyPanel v-else-if="section === 'permissoes'" :dot-id="dot.id" />
      <TriggersPanel v-else-if="section === 'gatilhos'" :dot-id="dot.id" />
      <TeamPanel v-else-if="section === 'equipe'" :dot-id="dot.id" />
      <MemoryPanel v-else-if="section === 'memoria'" :dot-id="dot.id" />
      <IntegrationPanel v-else-if="section === 'aplicativos'" :dot-id="dot.id" />
    </div>
    <ConfirmDialog :open="confirmDelete" title="Excluir DOT" :message="`Excluir “${dot.name}” apaga conversas, memoria e historico. Isso nao pode ser desfeito.`" confirm-label="Excluir" danger @confirm="remove" @cancel="confirmDelete = false" />
  </aside>
</template>
