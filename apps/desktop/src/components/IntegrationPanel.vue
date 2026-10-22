<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import FlexiSharePanel from "@/components/FlexiSharePanel.vue";
import { api, RuntimeError } from "@/api";
import { ACCOUNT_CONNECTION, AGENTS_CONNECTION, useAppsStore } from "@/stores/apps";
import { useSettingsStore } from "@/stores/settings";

const props = defineProps<{ dotId: string }>();
const dotId = props.dotId;
const apps = useAppsStore();
const settings = useSettingsStore();
const current = ref<string | "">("");
const error = ref<string | null>(null);
const saving = ref(false);
const usesAccountLogin = computed(() => typeof settings.items[ACCOUNT_CONNECTION] === "string");
const agentConnection = computed(() => apps.flexi.find((c) => c.id === settings.items[AGENTS_CONNECTION]));

onMounted(async () => {
  await apps.load();
  try { current.value = (await api.apps.integration(props.dotId))?.connection_id ?? ""; } catch (e) { error.value = e instanceof RuntimeError ? e.message : String(e); }
});
async function save() {
  saving.value = true;
  try { await api.apps.setIntegration(props.dotId, current.value || null); error.value = null; }
  catch (e) { error.value = e instanceof RuntimeError ? e.message : String(e); }
  finally { saving.value = false; }
}
</script>

<template>
  <div class="max-w-2xl space-y-4">
    <ErrorBanner :message="error ?? apps.error" />
    <section class="card space-y-3">
      <h3 class="text-sm font-semibold text-zinc-200">Flexi</h3>
      <template v-if="usesAccountLogin">
        <p v-if="agentConnection" class="text-xs text-zinc-400">Este DOT acessa o Flexi como <strong class="text-zinc-200">{{ agentConnection.user_name ?? agentConnection.email }}</strong>. A conta dos agentes é definida em Conectar aplicativos.</p>
        <p v-else class="text-xs text-amber-300">Nenhuma conta foi escolhida para os agentes. Este DOT está sem acesso ao Flexi.</p>
        <router-link to="/engines" class="text-xs text-indigo-400 hover:underline">Escolher conta dos agentes</router-link>
      </template>
      <template v-else>
        <p class="text-xs text-zinc-500">Qual usuario do Flexi este DOT usa. Tudo o que ele fizer no Flexi fica registrado no nome desse usuario.</p>
        <select v-model="current" class="input">
          <option value="">{{ apps.flexi.length === 1 ? `Automatico (${apps.flexi[0].label})` : apps.flexi.length ? "Nenhum (sem acesso ao Flexi)" : "Nenhuma conexao disponivel" }}</option>
          <option v-for="c in apps.flexi" :key="c.id" :value="c.id">{{ c.label }} · {{ c.user_name ?? c.email }}{{ c.status !== 'connected' ? ' (precisa reconectar)' : '' }}</option>
        </select>
        <div class="flex items-center justify-between">
          <router-link to="/engines" class="text-xs text-indigo-400 hover:underline">Gerenciar conexoes</router-link>
          <button class="btn-primary" :disabled="saving" @click="save">Salvar</button>
        </div>
      </template>
    </section>
    <FlexiSharePanel :dot-id="dotId" />
  </div>
</template>
