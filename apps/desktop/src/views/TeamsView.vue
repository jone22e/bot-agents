<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { ChevronDown, ChevronRight, Pencil, Plus, Trash2, X } from "lucide-vue-next";
import type { Team } from "@dots/shared-types";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import PageHeader from "@/components/PageHeader.vue";
import { useDismissalsStore } from "@/stores/dismissals";
import { useDotsStore } from "@/stores/dots";
import { useTeamsStore } from "@/stores/teams";

const teams = useTeamsStore();
const dots = useDotsStore();
const dismissals = useDismissalsStore();
const newName = ref("");
const newManager = ref("");
const renaming = ref<string | null>(null);
const renameTo = ref("");
const toDelete = ref<Team | null>(null);

onMounted(async () => { await teams.load(); if (!dots.items.length) await dots.load(); await Promise.allSettled(teams.items.map((t) => dismissals.load(t.id))); });
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
async function addMember(t: Team, ev: Event) {
  const el = ev.target as HTMLSelectElement;
  const id = el.value;
  el.value = "";
  if (id) await teams.addMember(t.id, id);
}
const soft = ["bg-violet-500/20 text-violet-300", "bg-emerald-500/20 text-emerald-300", "bg-amber-500/20 text-amber-300", "bg-sky-500/20 text-sky-300", "bg-rose-500/20 text-rose-300", "bg-teal-500/20 text-teal-300"];
const avatarCls = (id: string) => soft[[...id].reduce((n, c) => n + c.charCodeAt(0), 0) % soft.length];
const initial = (id: string) => name(id).charAt(0).toUpperCase();
</script>

<template>
  <div class="flex h-full flex-col">
    <PageHeader title="Equipes" subtitle="Um gerente delega aos funcionários da equipe dele. O gerente de uma equipe pode ser funcionário de outra, formando a hierarquia." />
    <div class="space-y-5 overflow-y-auto px-6 pb-8">
      <ErrorBanner :message="teams.error" />

      <div class="flex items-center gap-2 rounded-xl border border-zinc-800 bg-zinc-900/60 p-1.5">
        <input v-model="newName" class="h-9 min-w-0 flex-1 bg-transparent px-3 text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none" placeholder="Nova equipe, ex.: Mercado Livre" @keyup.enter="create" />
        <div class="relative shrink-0">
          <select v-model="newManager" class="h-9 cursor-pointer appearance-none rounded-lg bg-zinc-800 pl-3 pr-8 text-sm focus:outline-none" :class="newManager ? 'text-zinc-100' : 'text-zinc-400'"><option value="">Gerente (opcional)</option><option v-for="d in managerOptions(null)" :key="d.id" :value="d.id">{{ d.name }}</option></select>
          <ChevronDown :size="14" class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-400" />
        </div>
        <button class="h-9 shrink-0 rounded-lg bg-violet-500 px-4 text-sm font-semibold text-white hover:bg-violet-400 disabled:cursor-not-allowed disabled:opacity-50" :disabled="!newName.trim() || teams.busy" @click="create">Criar equipe</button>
      </div>

      <p v-if="!teams.loading && !teams.items.length" class="text-sm text-zinc-500">Nenhuma equipe ainda. Crie uma acima ou escolha "Gerente" ao criar um DOT.</p>

      <div class="flex flex-col gap-4">
        <section v-for="t in teams.items" :key="t.id" class="flex flex-col rounded-2xl border border-zinc-800 bg-zinc-900/60">
          <div class="space-y-4 p-5">
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div v-if="renaming === t.id" class="flex items-center gap-2"><input v-model="renameTo" class="input h-8" @keyup.enter="rename(t)" @keyup.esc="renaming = null" /><button class="btn-primary" @click="rename(t)">Salvar</button><button class="btn-secondary" @click="renaming = null">Cancelar</button></div>
                <h3 v-else class="flex items-center gap-1.5 text-[15px] font-semibold text-zinc-100"><span class="truncate">{{ t.name }}</span><button class="text-zinc-500 hover:text-zinc-300" title="Renomear" @click="renaming = t.id; renameTo = t.name"><Pencil :size="12" /></button></h3>
                <p class="mt-0.5 text-xs text-zinc-500">{{ parentOf(t) ? `Dentro de ${parentOf(t)!.name}` : "Equipe de topo" }} · {{ t.member_ids.length }} {{ t.member_ids.length === 1 ? "funcionário" : "funcionários" }}<template v-if="dismissals.forTeam(t.id).length"> · {{ dismissals.forTeam(t.id).length }} {{ dismissals.forTeam(t.id).length === 1 ? "demissão" : "demissões" }}</template></p>
              </div>
              <button class="shrink-0 rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-red-300" title="Excluir equipe" @click="toDelete = t"><Trash2 :size="14" /></button>
            </div>

            <div>
              <p class="mb-2 font-mono text-[11px] uppercase tracking-widest text-zinc-500">Gerente</p>
              <div v-if="t.manager_dot_id" class="flex items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2">
                <span class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-semibold" :class="avatarCls(t.manager_dot_id)">{{ initial(t.manager_dot_id) }}</span>
                <span class="min-w-0 flex-1 truncate text-sm font-medium text-zinc-100">{{ name(t.manager_dot_id) }}</span>
                <label class="relative shrink-0 cursor-pointer text-xs text-zinc-500 hover:text-zinc-300">Trocar
                  <select :value="t.manager_dot_id" class="absolute inset-0 cursor-pointer opacity-0" :disabled="teams.busy" @change="teams.setManager(t.id, ($event.target as HTMLSelectElement).value || null)">
                    <option value="">Sem gerente (ninguém delega)</option>
                    <option v-for="d in managerOptions(t)" :key="d.id" :value="d.id">{{ d.name }}</option>
                  </select>
                </label>
              </div>
              <div v-else class="relative">
                <select value="" class="h-10 w-full cursor-pointer appearance-none rounded-lg border border-dashed border-zinc-700 bg-transparent pl-3 pr-8 text-sm text-zinc-400 focus:outline-none" :disabled="teams.busy" @change="teams.setManager(t.id, ($event.target as HTMLSelectElement).value || null)">
                  <option value="">Sem gerente (ninguém delega)</option>
                  <option v-for="d in managerOptions(t)" :key="d.id" :value="d.id">{{ d.name }}</option>
                </select>
                <ChevronDown :size="14" class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-zinc-500" />
              </div>
            </div>

            <div>
              <p class="mb-2 font-mono text-[11px] uppercase tracking-widest text-zinc-500">Funcionários</p>
              <div class="flex flex-wrap items-center gap-2">
                <span v-for="id in t.member_ids" :key="id" class="flex items-center gap-2 rounded-full bg-zinc-800/80 py-1 pl-1 pr-2 text-sm text-zinc-200" :title="teams.managedBy(id) ? `Gerente de ${teams.managedBy(id)!.name}` : undefined">
                  <span class="flex h-6 w-6 items-center justify-center rounded-full text-[11px] font-semibold" :class="avatarCls(id)">{{ initial(id) }}</span>
                  {{ name(id) }}<span v-if="teams.managedBy(id)" class="text-xs text-zinc-500">· gerente de {{ teams.managedBy(id)!.name }}</span>
                  <button class="text-zinc-500 hover:text-red-300" title="Tirar da equipe" :disabled="teams.busy" @click="teams.removeMember(id)"><X :size="12" /></button>
                </span>
                <label v-if="memberOptions(t).length" class="relative flex cursor-pointer items-center gap-1.5 rounded-full border border-dashed border-violet-500/50 px-3 py-1 text-sm font-medium text-violet-300 hover:bg-violet-500/10"><Plus :size="13" /> Adicionar
                  <select class="absolute inset-0 cursor-pointer opacity-0" :disabled="teams.busy" @change="addMember(t, $event)"><option value="">Adicionar...</option><option v-for="d in memberOptions(t)" :key="d.id" :value="d.id">{{ d.name }}{{ teams.memberOf(d.id) ? ` (hoje em ${teams.memberOf(d.id)!.name})` : "" }}</option></select>
                </label>
              </div>
            </div>
          </div>

          <details v-if="dismissals.forTeam(t.id).length" class="group border-t border-zinc-800 px-5 py-3 text-xs">
            <summary class="flex cursor-pointer list-none items-center gap-1.5 font-medium [&::-webkit-details-marker]:hidden text-zinc-400 hover:text-zinc-200"><ChevronRight :size="13" class="transition-transform group-open:rotate-90" /> Demissões e lições <span class="font-mono text-zinc-500">{{ dismissals.forTeam(t.id).length }}</span></summary>
            <ul class="mt-2 space-y-1.5">
              <li v-for="d in dismissals.forTeam(t.id)" :key="d.id" class="text-zinc-300"><strong class="text-zinc-100">{{ d.dot_name }}</strong> <span class="text-zinc-500">· {{ new Date(d.created_at).toLocaleDateString() }} · por {{ d.manager_name }}</span><br />Motivo: {{ d.reason }}<span v-if="d.lesson && d.lesson !== d.reason" class="text-emerald-300"> · Lição: {{ d.lesson }}</span></li>
            </ul>
          </details>
        </section>
      </div>
    </div>
    <ConfirmDialog :open="toDelete !== null" title="Excluir equipe" :message="`Excluir a equipe “${toDelete?.name ?? ''}”? Os DOTs continuam existindo; o gerente deixa de delegar a eles.`" confirm-label="Excluir" danger @confirm="teams.remove(toDelete!.id); toDelete = null" @cancel="toDelete = null" />
  </div>
</template>
