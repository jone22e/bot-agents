<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Pencil, Trash2, UserMinus, UserPlus, Users } from "lucide-vue-next";
import type { Team } from "@dots/shared-types";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import PageHeader from "@/components/PageHeader.vue";
import { useDotsStore } from "@/stores/dots";
import { useTeamsStore } from "@/stores/teams";

const teams = useTeamsStore();
const dots = useDotsStore();
const newName = ref("");
const newManager = ref("");
const renaming = ref<string | null>(null);
const renameTo = ref("");
const addFor = reactive<Record<string, string>>({});
const toDelete = ref<Team | null>(null);

onMounted(async () => { await teams.load(); if (!dots.items.length) await dots.load(); });
const active = computed(() => dots.items.filter((d) => d.status !== "archived"));
const name = (id: string | null | undefined) => (id ? dots.items.find((d) => d.id === id)?.name ?? "?" : "");
const managerOptions = (t: Team | null) => active.value.filter((d) => !teams.managedBy(d.id) || teams.managedBy(d.id)?.id === t?.id);
const memberOptions = (t: Team) => active.value.filter((d) => d.id !== t.manager_dot_id && !t.member_ids.includes(d.id));
const parentOf = (t: Team) => (t.manager_dot_id ? teams.memberOf(t.manager_dot_id) : null);

async function create() {
  if (!newName.value.trim()) return;
  await teams.create({ name: newName.value.trim(), description: "", manager_dot_id: newManager.value || null, member_ids: [] });
  newName.value = "";
  newManager.value = "";
}
async function rename(t: Team) {
  if (renameTo.value.trim() && renameTo.value.trim() !== t.name) await teams.update(t.id, { name: renameTo.value.trim(), description: null });
  renaming.value = null;
}
async function addMember(t: Team) {
  const id = addFor[t.id];
  if (!id) return;
  await teams.addMember(t.id, id);
  addFor[t.id] = "";
}
</script>

<template>
  <div class="flex h-full flex-col">
    <PageHeader title="Equipes" subtitle="Um gerente delega aos funcionarios da equipe dele. O gerente de uma equipe pode ser funcionario de outra, formando a hierarquia." />
    <div class="max-w-3xl space-y-4 overflow-y-auto px-6 pb-8">
      <ErrorBanner :message="teams.error" />

      <div class="card flex items-center gap-2">
        <Users :size="18" class="shrink-0 text-indigo-300" />
        <input v-model="newName" class="input flex-1" placeholder="Nova equipe, ex.: Mercado Livre" @keyup.enter="create" />
        <select v-model="newManager" class="input w-56"><option value="">Gerente (opcional)</option><option v-for="d in managerOptions(null)" :key="d.id" :value="d.id">{{ d.name }}</option></select>
        <button class="btn-primary" :disabled="!newName.trim() || teams.busy" @click="create">Criar</button>
      </div>

      <p v-if="!teams.loading && !teams.items.length" class="text-sm text-zinc-500">Nenhuma equipe ainda. Crie uma acima ou escolha "Gerente" ao criar um DOT.</p>

      <section v-for="t in teams.items" :key="t.id" class="card space-y-3">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div v-if="renaming === t.id" class="flex items-center gap-2"><input v-model="renameTo" class="input h-9 w-64" @keyup.enter="rename(t)" @keyup.esc="renaming = null" /><button class="btn-primary" @click="rename(t)">Salvar</button><button class="btn-secondary" @click="renaming = null">Cancelar</button></div>
            <h3 v-else class="flex items-center gap-2 text-sm font-semibold text-zinc-100">{{ t.name }}<button class="btn-ghost" title="Renomear" @click="renaming = t.id; renameTo = t.name"><Pencil :size="13" /></button></h3>
            <p class="text-xs text-zinc-500">{{ parentOf(t) ? `Dentro de ${parentOf(t)!.name}` : "Equipe de topo" }} · {{ t.member_ids.length }} funcionario(s)</p>
          </div>
          <button class="btn-ghost text-red-300" title="Excluir equipe" @click="toDelete = t"><Trash2 :size="14" /></button>
        </div>
        <div class="flex items-center gap-2 text-sm">
          <span class="w-20 shrink-0 text-xs uppercase tracking-wide text-zinc-500">Gerente</span>
          <select :value="t.manager_dot_id ?? ''" class="input flex-1" :disabled="teams.busy" @change="teams.setManager(t.id, ($event.target as HTMLSelectElement).value || null)">
            <option value="">Sem gerente (ninguem delega)</option>
            <option v-for="d in managerOptions(t)" :key="d.id" :value="d.id">{{ d.name }}</option>
          </select>
        </div>
        <div class="flex items-start gap-2 text-sm">
          <span class="w-20 shrink-0 pt-1.5 text-xs uppercase tracking-wide text-zinc-500">Equipe</span>
          <div class="flex flex-1 flex-wrap items-center gap-2">
            <span v-for="id in t.member_ids" :key="id" class="flex items-center gap-1 rounded-full bg-zinc-800 py-1 pl-3 pr-1 text-xs text-zinc-200">{{ name(id) }}<span v-if="teams.managedBy(id)" class="text-zinc-500">· gerente de {{ teams.managedBy(id)!.name }}</span><button class="btn-ghost text-red-300" title="Tirar da equipe" :disabled="teams.busy" @click="teams.removeMember(id)"><UserMinus :size="12" /></button></span>
            <span class="flex items-center gap-1">
              <select v-model="addFor[t.id]" class="input h-8 w-56 text-xs"><option value="">Adicionar...</option><option v-for="d in memberOptions(t)" :key="d.id" :value="d.id">{{ d.name }}{{ teams.memberOf(d.id) ? ` (hoje em ${teams.memberOf(d.id)!.name})` : "" }}</option></select>
              <button class="btn-ghost" :disabled="!addFor[t.id] || teams.busy" @click="addMember(t)"><UserPlus :size="14" /></button>
            </span>
          </div>
        </div>
      </section>
    </div>
    <ConfirmDialog :open="toDelete !== null" title="Excluir equipe" :message="`Excluir a equipe “${toDelete?.name ?? ''}”? Os DOTs continuam existindo; o gerente deixa de delegar a eles.`" confirm-label="Excluir" danger @confirm="teams.remove(toDelete!.id); toDelete = null" @cancel="toDelete = null" />
  </div>
</template>
