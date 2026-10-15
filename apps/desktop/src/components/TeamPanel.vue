<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { UserMinus, UserPlus } from "lucide-vue-next";
import ErrorBanner from "@/components/ErrorBanner.vue";
import { useDotsStore } from "@/stores/dots";
import { useTeamsStore } from "@/stores/teams";

const props = defineProps<{ dotId: string }>();
const teams = useTeamsStore();
const dots = useDotsStore();
const router = useRouter();
const newTeam = ref("");
const addId = ref("");
const joinId = ref("");

onMounted(async () => { await teams.load(); if (!dots.items.length) await dots.load(); });
const manages = computed(() => teams.managedBy(props.dotId));
const memberOf = computed(() => teams.memberOf(props.dotId));
const name = (id: string | null | undefined) => (id ? dots.items.find((d) => d.id === id)?.name ?? "?" : "");
/** Candidatos a funcionario: DOTs ativos que nao sao este, nem ja estao na equipe. */
const candidates = computed(() => dots.items.filter((d) => d.id !== props.dotId && d.status !== "archived" && !manages.value?.member_ids.includes(d.id)));

async function createTeam() {
  if (!newTeam.value.trim()) return;
  await teams.create({ name: newTeam.value.trim(), description: "", manager_dot_id: props.dotId, member_ids: [] });
  newTeam.value = "";
}
async function addMember() {
  if (!manages.value || !addId.value) return;
  await teams.addMember(manages.value.id, addId.value);
  addId.value = "";
}
async function join() {
  if (!joinId.value) return;
  await teams.addMember(joinId.value, props.dotId);
  joinId.value = "";
}
</script>

<template>
  <div class="max-w-2xl space-y-5">
    <ErrorBanner :message="teams.error" />

    <section class="card space-y-3">
      <h3 class="text-sm font-semibold text-zinc-200">Gerente de</h3>
      <template v-if="manages">
        <p class="text-sm text-zinc-100">Equipe <strong>{{ manages.name }}</strong>. Ele pode pedir trabalho a estes funcionarios:</p>
        <ul class="divide-y divide-zinc-800 rounded-xl border border-zinc-800">
          <li v-for="id in manages.member_ids" :key="id" class="flex items-center justify-between px-3 py-2 text-sm text-zinc-200">{{ name(id) }}<button class="btn-ghost text-red-300" title="Tirar da equipe" :disabled="teams.busy" @click="teams.removeMember(id)"><UserMinus :size="14" /></button></li>
          <li v-if="!manages.member_ids.length" class="px-3 py-2 text-xs text-zinc-500">Nenhum funcionario ainda.</li>
        </ul>
        <div class="flex items-center gap-2">
          <select v-model="addId" class="input flex-1"><option value="">Adicionar funcionario...</option><option v-for="d in candidates" :key="d.id" :value="d.id">{{ d.name }}{{ teams.memberOf(d.id) ? ` (hoje em ${teams.memberOf(d.id)!.name})` : "" }}</option></select>
          <button class="btn-primary" :disabled="!addId || teams.busy" @click="addMember"><UserPlus :size="14" /></button>
        </div>
        <div class="flex justify-between text-xs"><router-link to="/teams" class="text-indigo-400 hover:underline">Gerenciar equipes</router-link><button class="text-red-300 hover:underline" :disabled="teams.busy" @click="teams.setManager(manages!.id, null)">Deixar de ser gerente</button></div>
      </template>
      <template v-else>
        <p class="text-xs text-zinc-500">Nao gerencia nenhuma equipe. Para que ele delegue trabalho, crie uma equipe com ele como gerente ou assuma uma equipe sem gerente.</p>
        <div class="flex items-center gap-2">
          <input v-model="newTeam" class="input flex-1" placeholder="nome da nova equipe" @keyup.enter="createTeam" />
          <button class="btn-primary" :disabled="!newTeam.trim() || teams.busy" @click="createTeam">Criar equipe</button>
        </div>
        <div v-if="teams.withoutManager.length" class="flex flex-wrap gap-2">
          <button v-for="t in teams.withoutManager" :key="t.id" class="rounded-full bg-zinc-800 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-700" :disabled="teams.busy" @click="teams.setManager(t.id, dotId)">Assumir "{{ t.name }}"</button>
        </div>
      </template>
    </section>

    <section class="card space-y-3">
      <h3 class="text-sm font-semibold text-zinc-200">Funcionario de</h3>
      <template v-if="memberOf">
        <p class="text-sm text-zinc-100">{{ name(memberOf.manager_dot_id) || "Ninguem ainda" }} <span class="text-zinc-500">(equipe {{ memberOf.name }})</span></p>
        <div class="flex justify-end"><button class="text-xs text-red-300 hover:underline" :disabled="teams.busy" @click="teams.removeMember(dotId)">Sair da equipe</button></div>
      </template>
      <template v-else>
        <p class="text-xs text-zinc-500">Nao faz parte de nenhuma equipe como funcionario.</p>
        <div class="flex items-center gap-2">
          <select v-model="joinId" class="input flex-1"><option value="">Entrar na equipe...</option><option v-for="t in teams.items.filter((t) => t.id !== manages?.id)" :key="t.id" :value="t.id">{{ t.name }}{{ t.manager_dot_id ? ` · gerente: ${name(t.manager_dot_id)}` : " · sem gerente" }}</option></select>
          <button class="btn-primary" :disabled="!joinId || teams.busy" @click="join">Entrar</button>
        </div>
      </template>
      <p class="text-xs text-zinc-500">Cada DOT trabalha com as proprias permissoes; nada e herdado de quem pediu. Pedidos entre DOTs aparecem em Atividades.</p>
    </section>
    <p class="text-xs text-zinc-500">Todas as equipes: <button class="text-indigo-400 hover:underline" @click="router.push('/teams')">abrir Equipes</button></p>
  </div>
</template>
