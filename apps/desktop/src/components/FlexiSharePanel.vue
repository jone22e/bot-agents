<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Search } from "lucide-vue-next";
import type { FlexiShare, FlexiShareUser } from "@dots/shared-types";
import ErrorBanner from "@/components/ErrorBanner.vue";
import Toggle from "@/components/Toggle.vue";
import { api, RuntimeError, type FlexiUser } from "@/api";
import { useDotsStore } from "@/stores/dots";

/**
 * Compartilhar um DOT no Flexi web (ADR 0016): interruptor + usuarios escolhidos. Os escolhidos veem o DOT
 * no icone "Agentes DOTS" do Flexi e conversam com ele numa conversa unica, tipo grupo.
 */
const props = defineProps<{ dotId: string }>();
const dots = useDotsStore();
const dot = computed(() => dots.byId(props.dotId));
const enabled = ref(false);
const selected = ref<FlexiShareUser[]>([]);
const users = ref<FlexiUser[] | null>(null);
const loadingUsers = ref(false);
const query = ref("");
const error = ref<string | null>(null);
const saving = ref(false);
const saved = ref(false);

function reset() {
  const share = dot.value?.flexi_share;
  enabled.value = share?.enabled ?? false;
  selected.value = share ? share.users.map((u) => ({ ...u })) : [];
}
watch(dot, reset, { immediate: true });

const dirty = computed(() => {
  const cur = dot.value?.flexi_share;
  if (!cur) return false;
  if (cur.enabled !== enabled.value) return true;
  const a = cur.users.map((u) => u.id).sort().join(",");
  const b = selected.value.map((u) => u.id).sort().join(",");
  return a !== b;
});

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  const list = users.value ?? [];
  return q ? list.filter((u) => u.nome.toLowerCase().includes(q) || u.email.toLowerCase().includes(q)) : list;
});

function isSelected(id: number) { return selected.value.some((u) => u.id === id); }
function toggleUser(u: FlexiUser) {
  if (isSelected(u.id)) selected.value = selected.value.filter((s) => s.id !== u.id);
  else selected.value = [...selected.value, { id: u.id, name: u.nome }];
}

async function loadUsers() {
  if (users.value || loadingUsers.value) return;
  loadingUsers.value = true;
  try { users.value = await api.apps.flexiUsers(); error.value = null; }
  catch (e) { error.value = e instanceof RuntimeError ? e.message : String(e); }
  finally { loadingUsers.value = false; }
}
onMounted(() => { if (enabled.value) void loadUsers(); });
watch(enabled, (on) => { if (on) void loadUsers(); });

async function save() {
  saving.value = true;
  saved.value = false;
  try {
    const flexi_share: FlexiShare = { enabled: enabled.value, users: enabled.value ? selected.value : [] };
    await dots.update(props.dotId, { flexi_share });
    error.value = null;
    saved.value = true;
  } catch (e) { error.value = e instanceof RuntimeError ? e.message : String(e); }
  finally { saving.value = false; }
}
</script>

<template>
  <section class="card space-y-3" data-testid="flexi-share">
    <h3 class="text-sm font-semibold text-zinc-200">Compartilhar no Flexi</h3>
    <ErrorBanner :message="error" />
    <Toggle v-model="enabled" label="Compartilhar este DOT no Flexi web" description="Os usuários escolhidos veem o DOT no Flexi e conversam com ele numa conversa única, como um grupo. Se este DOT é gerente, os funcionários da equipe dele ficam compartilhados com as mesmas pessoas. Desligado, só você vê o DOT no Flexi." />
    <template v-if="enabled">
      <p class="text-xs text-zinc-500">Quem pode falar com este DOT pelo Flexi. As aprovações continuam com você.</p>
      <div v-if="selected.length" class="flex flex-wrap gap-1.5">
        <button v-for="u in selected" :key="u.id" type="button" class="inline-flex items-center gap-1 rounded-full border border-violet-500/40 bg-violet-500/10 px-2.5 py-0.5 text-xs text-violet-200 hover:bg-violet-500/20" :title="`Remover ${u.name}`" @click="selected = selected.filter((s) => s.id !== u.id)">{{ u.name || `#${u.id}` }} ×</button>
      </div>
      <p v-else class="text-xs text-amber-300">Nenhum usuário escolhido: ninguém verá este DOT no Flexi até você marcar alguém.</p>
      <div class="relative">
        <Search :size="14" class="pointer-events-none absolute left-2.5 top-2.5 text-zinc-500" />
        <input v-model="query" class="input pl-8" placeholder="Buscar usuário por nome ou e-mail" @focus="loadUsers" />
      </div>
      <p v-if="loadingUsers" class="text-xs text-zinc-500">Carregando usuários do Flexi…</p>
      <ul v-else-if="users" class="max-h-56 divide-y divide-zinc-800 overflow-y-auto rounded-lg border border-zinc-800">
        <li v-if="!filtered.length" class="px-3 py-2 text-xs text-zinc-500">Nenhum usuário encontrado.</li>
        <li v-for="u in filtered" :key="u.id">
          <label class="flex cursor-pointer items-center gap-3 px-3 py-2 hover:bg-zinc-800/60">
            <input type="checkbox" class="h-4 w-4 accent-violet-500" :checked="isSelected(u.id)" @change="toggleUser(u)" />
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm text-zinc-100">{{ u.nome }}</span>
              <span class="block truncate text-xs text-zinc-500">{{ u.email }}</span>
            </span>
          </label>
        </li>
      </ul>
    </template>
    <div class="flex items-center justify-between">
      <span class="text-xs text-zinc-500">{{ saved && !dirty ? "Salvo. O Flexi é atualizado em segundos." : "" }}</span>
      <button class="btn-primary" :disabled="saving || !dirty" @click="save">Salvar</button>
    </div>
  </section>
</template>
