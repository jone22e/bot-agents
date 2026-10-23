<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { Check, ChevronDown, Search } from "lucide-vue-next";
import type { EngineKind } from "@dots/shared-types";
import { MODEL_PLACEHOLDER, modelGroups, modelLabel } from "@/lib/models";
import { useEnginesStore } from "@/stores/engines";

/**
 * Seletor de modelo (estilo Ovseer): botao com o atual e lista flutuante com busca.
 * Vazio = padrao (`emptyLabel`). "Outro" abre um campo para digitar o ID.
 * Codex e Antigravity listam o que o CLI informou na ultima deteccao; o Claude tem lista fixa.
 */
const props = defineProps<{ engine: EngineKind | ""; modelValue: string; emptyLabel: string; disabled?: boolean; id?: string }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const CUSTOM = "__custom__";
const engines = useEnginesStore();
const catalog = computed(() => (props.engine ? engines.byKind(props.engine)?.last_detection?.models ?? [] : []));
const groups = computed(() => modelGroups(props.engine, catalog.value));
const noCatalog = computed(() => props.engine !== "" && props.engine !== "claude" && !catalog.value.length);
const knownIds = computed(() => new Set(groups.value.flatMap((g) => g.items.map((m) => m.id))));
const customMode = ref(false);
const custom = ref("");
// o valor pode mudar por fora (troca de assistente, carga do DOT): o campo "Outro" segue o estado real
watch([() => props.modelValue, knownIds], ([m, ids]) => {
  if (!m) { if (!customMode.value) custom.value = ""; return; }
  if (ids.has(m)) { customMode.value = false; custom.value = ""; }
  else { customMode.value = true; if (custom.value.trim() !== m) custom.value = m; }
}, { immediate: true });

const label = computed(() => (customMode.value ? "Outro (digitar o ID)" : props.modelValue ? modelLabel(props.engine, props.modelValue, catalog.value) : props.emptyLabel));
function onCustomInput(e: Event) {
  custom.value = (e.target as HTMLInputElement).value;
  emit("update:modelValue", custom.value.trim());
}

// ---------- lista com busca ----------
const open = ref(false);
const btn = ref<HTMLElement>();
const listEl = ref<HTMLElement>();
const searchEl = ref<HTMLInputElement>();
const query = ref("");
const index = ref(0);
const style = ref<Record<string, string>>({});
const plain = (t: string) => t.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
const filtered = computed(() => {
  const words = plain(query.value).split(/\s+/).filter(Boolean);
  if (!words.length) return groups.value;
  return groups.value
    .map((g) => ({ label: g.label, items: g.items.filter((m) => words.every((w) => plain(`${m.label} ${m.id} ${m.description ?? ""}`).includes(w))) }))
    .filter((g) => g.items.length);
});
const flat = computed(() => filtered.value.flatMap((g) => g.items.map((m) => m.id)));
const at = (id: string) => flat.value.indexOf(id);
/** Indices das duas opcoes fixas do fim (padrao, outro) */
const extra = (n: number) => flat.value.length + n;

function place() {
  const r = btn.value?.getBoundingClientRect();
  if (!r) return;
  const below = window.innerHeight - r.bottom - 12;
  const up = below < 260 && r.top > below;
  style.value = {
    left: `${r.left}px`, width: `${Math.max(r.width, 280)}px`,
    ...(up ? { bottom: `${window.innerHeight - r.top + 6}px`, maxHeight: `${Math.min(420, r.top - 12)}px` } : { top: `${r.bottom + 6}px`, maxHeight: `${Math.min(420, below)}px` }),
  };
}
function toggle() {
  if (props.disabled) return;
  open.value = !open.value;
  if (!open.value) return;
  query.value = "";
  index.value = customMode.value ? extra(1) : props.modelValue ? Math.max(0, at(props.modelValue)) : extra(0);
  place();
  nextTick(() => { searchEl.value?.focus(); listEl.value?.querySelector(".cur")?.scrollIntoView({ block: "nearest" }); });
}
function pick(id: string) {
  if (id === CUSTOM) { customMode.value = true; custom.value = ""; emit("update:modelValue", ""); }
  else { customMode.value = false; custom.value = ""; emit("update:modelValue", id); }
  open.value = false;
}
function onKey(e: KeyboardEvent) {
  const n = flat.value.length + 2;
  if (e.key === "Escape") { open.value = false; return; }
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    index.value = (index.value + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    nextTick(() => listEl.value?.querySelector(".cur")?.scrollIntoView({ block: "nearest" }));
  } else if (e.key === "Enter") {
    e.preventDefault();
    const i = index.value;
    pick(i < flat.value.length ? flat.value[i] : i === extra(0) ? "" : CUSTOM);
  }
}
watch(query, () => (index.value = 0));
watch(open, (o) => { if (o) window.addEventListener("resize", place); else window.removeEventListener("resize", place); });
const onDoc = (e: MouseEvent) => {
  const t = e.target as Node;
  if (open.value && !btn.value?.contains(t) && !listEl.value?.contains(t)) open.value = false;
};
onMounted(() => document.addEventListener("mousedown", onDoc));
onUnmounted(() => { document.removeEventListener("mousedown", onDoc); window.removeEventListener("resize", place); });
</script>

<template>
  <div class="space-y-2">
    <button ref="btn" :id="id" type="button" class="input flex items-center justify-between gap-2 text-left disabled:cursor-not-allowed disabled:opacity-50" :class="open ? 'border-indigo-500' : ''" :disabled="disabled" @click="toggle">
      <span class="truncate" :class="modelValue || customMode ? 'text-zinc-100' : 'text-zinc-500'">{{ label }}</span>
      <ChevronDown :size="14" class="shrink-0 text-zinc-500" />
    </button>
    <Teleport to="body">
      <div v-if="open" ref="listEl" class="fixed z-50 flex flex-col overflow-hidden rounded-xl border border-zinc-700 bg-zinc-900 shadow-2xl" :style="style" role="listbox">
        <label class="flex h-10 shrink-0 items-center gap-2 border-b border-zinc-800 px-3">
          <Search :size="13" class="text-zinc-500" />
          <input ref="searchEl" v-model="query" type="text" class="min-w-0 flex-1 bg-transparent text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none" placeholder="Buscar modelo" spellcheck="false" autocomplete="off" @keydown="onKey" />
        </label>
        <div class="min-h-0 overflow-auto p-1.5">
          <template v-for="g in filtered" :key="g.label">
            <h6 class="mx-2 mb-1 mt-2 text-[10.5px] font-semibold uppercase tracking-wider text-zinc-500">{{ g.label }}</h6>
            <button v-for="m in g.items" :key="m.id" type="button" class="flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-left text-sm" :class="[at(m.id) === index ? 'cur bg-zinc-800' : '', !customMode && modelValue === m.id ? 'text-indigo-400' : 'text-zinc-100']" @mouseenter="index = at(m.id)" @click="pick(m.id)">
              <span class="shrink-0 font-medium">{{ m.label }}</span>
              <span v-if="m.description" class="min-w-0 flex-1 truncate text-xs text-zinc-500">{{ m.description }}</span>
              <span v-else class="flex-1"></span>
              <Check v-if="!customMode && modelValue === m.id" :size="13" class="shrink-0 text-indigo-400" />
            </button>
          </template>
          <p v-if="groups.length && !filtered.length" class="m-2 text-xs text-zinc-500">Nenhum modelo com esse nome.</p>
          <p v-if="noCatalog" class="m-2 text-xs text-zinc-500">A lista de modelos ainda nao esta disponivel. Deixe no padrao do assistente ou clique em "Verificar de novo".</p>
          <hr v-if="groups.length" class="mx-1 my-1.5 border-zinc-800" />
          <button type="button" class="flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-left text-sm" :class="[index === extra(0) ? 'cur bg-zinc-800' : '', !customMode && !modelValue ? 'text-indigo-400' : 'text-zinc-100']" @mouseenter="index = extra(0)" @click="pick('')">
            <span class="flex-1 truncate font-medium">{{ emptyLabel }}</span>
            <Check v-if="!customMode && !modelValue" :size="13" class="shrink-0 text-indigo-400" />
          </button>
          <button type="button" class="flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-left text-sm" :class="[index === extra(1) ? 'cur bg-zinc-800' : '', customMode ? 'text-indigo-400' : 'text-zinc-100']" @mouseenter="index = extra(1)" @click="pick(CUSTOM)">
            <span class="flex-1 truncate font-medium">Outro (digitar o ID)…</span>
            <Check v-if="customMode" :size="13" class="shrink-0 text-indigo-400" />
          </button>
        </div>
      </div>
    </Teleport>
    <input v-if="customMode" :value="custom" type="text" class="input font-mono text-xs" :placeholder="engine ? MODEL_PLACEHOLDER[engine] : ''" spellcheck="false" :disabled="disabled" @input="onCustomInput" />
  </div>
</template>
