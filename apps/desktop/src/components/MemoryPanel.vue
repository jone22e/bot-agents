<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Search } from "lucide-vue-next";
import type { MemoryKind } from "@dots/shared-types";
import ErrorBanner from "@/components/ErrorBanner.vue";
import { useMemoriesStore } from "@/stores/memories";

const props = defineProps<{ dotId: string }>();
const store = useMemoriesStore();
const newContent = ref("");
const editingId = ref<string | null>(null);
const editContent = ref("");
const kinds: { value: MemoryKind | null; label: string }[] = [
  { value: null, label: "Tudo" }, { value: "long_term", label: "Aprendido" }, { value: "history", label: "Trabalhos passados" }, { value: "working", label: "Em andamento" },
];
const kindLabel: Record<string, string> = { long_term: "aprendido", working: "em andamento", history: "trabalho passado", identity: "identidade" };
/** Filtros so fazem sentido quando ha algo para filtrar (ou quando um filtro ja esta ativo). */
const showFilters = computed(() => store.items.length > 0 || store.kind !== null || store.query !== "");

onMounted(() => { store.kind = null; store.query = ""; store.load(props.dotId); });
async function create() {
  if (!newContent.value.trim()) return;
  await store.create({ dot_id: props.dotId, kind: "long_term", content: newContent.value.trim(), importance: 3, source: "user" });
  newContent.value = "";
}
function setKind(k: MemoryKind | null) { store.kind = k; store.load(); }
async function saveEdit(id: string) {
  await store.update(id, { content: editContent.value });
  editingId.value = null;
}
</script>

<template>
  <div class="max-w-3xl space-y-4">
    <ErrorBanner :message="store.error" />
    <section>
      <label class="label" for="teach">Ensinar algo ao DOT</label>
      <textarea id="teach" v-model="newContent" class="input" rows="2" placeholder="Ex.: Os testes rodam com npm test. O cliente se chama Flexi." @keydown.meta.enter="create" @keydown.ctrl.enter="create"></textarea>
      <div class="mt-2 flex items-center justify-between gap-3">
        <p class="text-xs text-zinc-500">Ele vai lembrar disso em todos os trabalhos.</p>
        <button class="btn-primary shrink-0" :disabled="!newContent.trim()" @click="create">Ensinar</button>
      </div>
    </section>

    <template v-if="showFilters">
      <div class="flex flex-wrap gap-1.5">
        <button v-for="k in kinds" :key="String(k.value)" class="rounded-full px-3 py-1 text-xs" :class="store.kind === k.value ? 'bg-indigo-600 text-white' : 'bg-zinc-800 text-zinc-300 hover:bg-zinc-700'" @click="setKind(k.value)">{{ k.label }}</button>
      </div>
      <div class="relative">
        <Search :size="14" class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
        <input v-model="store.query" class="input pl-9" placeholder="Buscar e apertar Enter" @keyup.enter="store.load()" />
      </div>
    </template>

    <p v-if="!store.loading && !store.items.length" class="text-xs text-zinc-500">{{ store.query || store.kind ? "Nada encontrado com esse filtro." : "Nada guardado ainda. O DOT aprende com os trabalhos e com o que voce ensinar aqui." }}</p>
    <ul class="space-y-2">
      <li v-for="m in store.items" :key="m.id" class="card">
        <template v-if="editingId === m.id">
          <textarea v-model="editContent" class="input" rows="3"></textarea>
          <div class="mt-2 flex justify-end gap-2"><button class="btn-ghost" @click="editingId = null">Cancelar</button><button class="btn-primary" @click="saveEdit(m.id)">Salvar</button></div>
        </template>
        <template v-else>
          <p class="whitespace-pre-wrap text-sm text-zinc-200">{{ m.content }}</p>
          <div class="mt-2 flex items-center justify-between gap-2 text-xs text-zinc-500">
            <span class="truncate">{{ kindLabel[m.kind] }} · {{ new Date(m.updated_at).toLocaleDateString() }}</span>
            <span class="flex shrink-0 gap-3">
              <button class="text-indigo-400 hover:underline" @click="editingId = m.id; editContent = m.content">editar</button>
              <button class="text-red-300 hover:underline" @click="store.remove(m.id)">excluir</button>
            </span>
          </div>
        </template>
      </li>
    </ul>
  </div>
</template>
