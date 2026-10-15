<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { Dot, DotCreate, DotUpdate } from "@dots/shared-types";
import DotForm from "@/components/DotForm.vue";
import NewDotWizard from "@/components/NewDotWizard.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import PageHeader from "@/components/PageHeader.vue";
import MemoryPanel from "@/components/MemoryPanel.vue";
import PolicyPanel from "@/components/PolicyPanel.vue";
import TeamPanel from "@/components/TeamPanel.vue";
import TriggersPanel from "@/components/TriggersPanel.vue";
import { api, RuntimeError } from "@/api";
import { useDotsStore } from "@/stores/dots";
import { useEnginesStore } from "@/stores/engines";

const route = useRoute();
const router = useRouter();
const dots = useDotsStore();
const engines = useEnginesStore();
const id = computed(() => (route.params.id as string | undefined) ?? null);
const dot = ref<Dot | null>(null);
const loading = ref(false);
const saving = ref(false);
const error = ref<string | null>(null);
type Tab = "geral" | "permissoes" | "gatilhos" | "equipe" | "memoria";
const tab = ref<Tab>((route.query.tab as Tab) ?? "geral");

onMounted(async () => {
  if (engines.items.length === 0) await engines.load();
  if (id.value) {
    loading.value = true;
    try { dot.value = dots.byId(id.value) ?? (await api.dots.get(id.value)); }
    catch (e) { error.value = e instanceof RuntimeError ? e.message : String(e); }
    finally { loading.value = false; }
  }
});

async function onCreate(input: DotCreate, trust: "read" | "write" | "exec") {
  saving.value = true; error.value = null;
  try {
    const d = await dots.create(input);
    await api.policies.set(d.id, { auto_approve_level: trust });
    router.push(`/chat/${d.id}`);
  } catch (e) { error.value = e instanceof RuntimeError ? e.message : String(e); }
  finally { saving.value = false; }
}
async function onUpdate(patch: DotUpdate) {
  if (!id.value) return;
  saving.value = true; error.value = null;
  try { await dots.update(id.value, patch); router.push(`/chat/${id.value}`); }
  catch (e) { error.value = e instanceof RuntimeError ? e.message : String(e); }
  finally { saving.value = false; }
}
</script>

<template>
  <NewDotWizard v-if="!id" />
  <div v-else class="flex h-full flex-col">
    <PageHeader title="Editar DOT" />
    <div class="overflow-y-auto px-6 pb-8">
    <ErrorBanner :message="error" />
    <div v-if="id" class="mb-5 flex gap-1 border-b border-zinc-800">
      <button v-for="t in ([['geral','Sobre o DOT'],['permissoes','Permissoes'],['gatilhos','Gatilhos'],['equipe','Equipe'],['memoria','Memoria']] as const)" :key="t[0]" class="-mb-px border-b-2 px-3 py-2 text-sm" :class="tab === t[0] ? 'border-indigo-500 text-zinc-100' : 'border-transparent text-zinc-400 hover:text-zinc-200'" @click="tab = t[0]">{{ t[1] }}</button>
      <router-link v-if="dot" :to="`/chat/${dot.id}`" class="ml-auto self-center text-xs text-indigo-400 hover:underline">Conversar com {{ dot.name }}</router-link>
    </div>
    <p v-if="loading" class="text-sm text-zinc-400">Carregando...</p>
    <template v-else-if="tab === 'geral'">
      <DotForm v-if="!id || dot" :dot="dot" :saving="saving" @create="onCreate" @update="onUpdate" @cancel="router.push(id ? `/chat/${id}` : '/')" />
    </template>
    <PolicyPanel v-else-if="tab === 'permissoes' && id" :dot-id="id" />
    <TriggersPanel v-else-if="tab === 'gatilhos' && id" :dot-id="id" />
    <TeamPanel v-else-if="tab === 'equipe' && id" :dot-id="id" />
    <MemoryPanel v-else-if="tab === 'memoria' && id" :dot-id="id" />
    </div>
  </div>
</template>
