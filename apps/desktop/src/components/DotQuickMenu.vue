<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { CalendarClock, ChevronRight, Cpu, FileText, Pause, Play } from "lucide-vue-next";
import type { Dot } from "@dots/shared-types";
import Avatar from "@/components/Avatar.vue";
import ModelPicker from "@/components/ModelPicker.vue";
import { ENGINE_LABEL, describeSchedule } from "@/lib/humanize";
import { modelLabel } from "@/lib/models";
import { useAutomationStore } from "@/stores/automation";
import { useContactsStore } from "@/stores/contacts";
import { useDotsStore } from "@/stores/dots";
import { useEnginesStore } from "@/stores/engines";

/**
 * Menu rapido que abre ao clicar no nome do DOT no topo da conversa: estado, executar agora, pausar/retomar,
 * trocar o modelo sem sair da conversa, agendamento, instrucoes e o perfil completo. Fecha com Esc ou clique fora.
 */
export type ProfileSection = "sobre" | "gatilhos" | null;
const props = defineProps<{ dot: Dot; busy?: boolean }>();
const emit = defineEmits<{ close: []; run: []; profile: [section: ProfileSection] }>();
const dots = useDotsStore();
const contacts = useContactsStore();
const engines = useEnginesStore();
const automation = useAutomationStore();
const el = ref<HTMLElement | null>(null);
const changingModel = ref(false);
const savingModel = ref(false);

const current = computed(() => dots.byId(props.dot.id) ?? props.dot);
const contact = computed(() => contacts.byDot(props.dot.id));
const engineCfg = computed(() => (current.value.primary_engine ? engines.byKind(current.value.primary_engine) : null));
const catalog = computed(() => engineCfg.value?.last_detection?.models ?? []);
const modelText = computed(() => {
  const engine = current.value.primary_engine;
  if (!engine) return "Sem assistente";
  const id = current.value.model || engineCfg.value?.model;
  return [id ? modelLabel(engine, id, catalog.value) : "Modelo padrao", ENGINE_LABEL[engine]].join(" · ");
});
/** Primeiro agendamento ativo deste DOT (o perfil mostra todos). */
const schedule = computed(() => (automation.dotId === props.dot.id ? automation.schedules.find((s) => s.enabled) ?? null : null));
const scheduleText = computed(() => (schedule.value ? describeSchedule(schedule.value) : "Nenhum"));
const statusText = computed(() => {
  if (current.value.status === "paused") return "Pausado";
  if (props.busy || contact.value?.state === "busy") return `Trabalhando · ${contact.value?.state_text ?? "em andamento"}`;
  const next = schedule.value?.next_run_at ? nextIn(schedule.value.next_run_at) : null;
  return next ? `Ativo · proxima rodada ${next}` : "Ativo";
});
function nextIn(iso: string) {
  const min = Math.round((new Date(iso).getTime() - Date.now()) / 60000);
  if (min <= 0) return "agora";
  if (min < 60) return `em ${min} min`;
  const h = Math.floor(min / 60);
  return h < 24 ? `em ${h} h` : `em ${Math.round(h / 24)} dia(s)`;
}

async function setModel(id: string) {
  savingModel.value = true;
  try { await dots.update(props.dot.id, { model: id || null }); changingModel.value = false; } finally { savingModel.value = false; }
}
function toggle() {
  const id = props.dot.id;
  return current.value.status === "paused" ? dots.resume(id) : dots.pause(id);
}

function onKey(e: KeyboardEvent) { if (e.key === "Escape") emit("close"); }
function onPointer(e: Event) {
  const t = e.target as HTMLElement;
  // o seletor de modelo flutua fora do menu (Teleport): clicar nele nao fecha
  if (el.value && !el.value.contains(t) && !t.closest("[data-model-picker]")) emit("close");
}
onMounted(() => {
  void automation.load(props.dot.id);
  window.addEventListener("keydown", onKey);
  window.addEventListener("mousedown", onPointer, true);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  window.removeEventListener("mousedown", onPointer, true);
});
</script>

<template>
  <div ref="el" role="menu" :aria-label="`Menu de ${current.name}`" class="pop absolute left-1/2 top-full z-40 mt-1 w-[330px] -translate-x-1/2 rounded-2xl border border-zinc-700/80 bg-zinc-950/95 p-3 text-sm text-zinc-100 shadow-2xl backdrop-blur" data-testid="dot-quick-menu">
    <div class="flex items-center gap-3 px-1">
      <span class="rounded-full ring-2" :class="current.status === 'paused' ? 'ring-amber-400' : 'ring-emerald-500'"><Avatar :name="current.name" :seed="current.id" size="md" /></span>
      <div class="min-w-0 flex-1">
        <p class="truncate font-semibold">{{ current.name }}</p>
        <p class="flex items-center gap-1.5 truncate text-xs text-zinc-400"><span class="inline-block h-1.5 w-1.5 shrink-0 rounded-full" :class="current.status === 'paused' ? 'bg-amber-400' : 'bg-emerald-400'" />{{ statusText }}</p>
      </div>
    </div>
    <div class="mt-3 grid grid-cols-2 gap-2">
      <button type="button" class="flex items-center justify-center gap-2 rounded-xl bg-violet-500 py-2.5 text-sm font-semibold text-white hover:bg-violet-400 disabled:cursor-not-allowed disabled:opacity-50" :disabled="busy || current.status === 'paused'" :title="current.status === 'paused' ? 'Retome o DOT para executar' : busy ? 'Ja esta trabalhando' : schedule ? 'Roda agora o pedido do agendamento' : 'Pede que siga o objetivo definido'" data-action="run" @click="emit('run')"><Play :size="15" /> Executar agora</button>
      <button v-if="current.status === 'paused'" type="button" class="flex items-center justify-center gap-2 rounded-xl bg-zinc-800 py-2.5 text-sm font-semibold hover:bg-zinc-700" data-action="resume" @click="toggle"><Play :size="15" /> Retomar</button>
      <button v-else type="button" class="flex items-center justify-center gap-2 rounded-xl bg-zinc-800 py-2.5 text-sm font-semibold hover:bg-zinc-700" data-action="pause" @click="toggle"><Pause :size="15" /> Pausar</button>
    </div>
    <div class="mt-3 divide-y divide-zinc-800 overflow-hidden rounded-xl border border-zinc-800">
      <div class="px-3 py-2.5">
        <div class="flex items-center gap-3">
          <Cpu :size="17" class="shrink-0 text-zinc-400" />
          <span class="min-w-0 flex-1">
            <span class="block text-sm">Modelo</span>
            <span class="block truncate text-xs text-zinc-400">{{ modelText }}</span>
          </span>
          <button type="button" class="shrink-0 text-xs font-semibold text-violet-300 hover:underline disabled:opacity-50" :disabled="!current.primary_engine || savingModel" data-action="change-model" @click="changingModel = !changingModel">{{ changingModel ? "Fechar" : "Trocar" }}</button>
        </div>
        <div v-if="changingModel" class="mt-2" data-model-picker>
          <ModelPicker :engine="current.primary_engine ?? ''" :model-value="current.model ?? ''" :disabled="savingModel" :empty-label="engineCfg?.model ? `${modelLabel(current.primary_engine ?? '', engineCfg.model, catalog)} (padrao em Configuracoes)` : 'Modelo padrao do assistente'" @update:model-value="setModel" />
        </div>
      </div>
      <button type="button" class="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-zinc-900" data-action="schedule" @click="emit('profile', 'gatilhos')">
        <CalendarClock :size="17" class="shrink-0 text-zinc-400" />
        <span class="min-w-0 flex-1">
          <span class="block text-sm">Agendamento</span>
          <span class="block truncate text-xs text-zinc-400">{{ scheduleText }}</span>
        </span>
        <ChevronRight :size="15" class="shrink-0 text-zinc-600" />
      </button>
      <button type="button" class="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-zinc-900" data-action="instructions" @click="emit('profile', 'sobre')">
        <FileText :size="17" class="shrink-0 text-zinc-400" />
        <span class="flex-1 text-sm">Editar instruções</span>
        <ChevronRight :size="15" class="shrink-0 text-zinc-600" />
      </button>
    </div>
    <button type="button" class="mt-2 w-full py-1.5 text-center text-xs font-medium text-violet-300 hover:underline" data-action="profile" @click="emit('profile', null)">Ver perfil completo</button>
  </div>
</template>
