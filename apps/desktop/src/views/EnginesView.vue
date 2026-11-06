<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import PageHeader from "@/components/PageHeader.vue";
import FlexiCard from "@/components/FlexiCard.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import { api } from "@/api";
import { useSettingsStore } from "@/stores/settings";

const settings = useSettingsStore();
const mode = computed(() => settings.items["browser.mode"] === "personal_chrome" ? "personal_chrome" : "isolated");
const saving = ref(false);
const error = ref<string | null>(null);
const extension = ref<{ path: string; extension_id: string | null; connected: boolean } | null>(null);
const extensionId = ref("");
async function loadExtension(id?: string) {
  try {
    extension.value = await api.browser.personalSetup(id);
    extensionId.value = extension.value.extension_id ?? "";
    error.value = null;
  } catch (e) { error.value = e instanceof Error ? e.message : String(e); }
}
onMounted(() => { if (mode.value === "personal_chrome") void loadExtension(); });
async function selectMode(value: "isolated" | "personal_chrome") {
  if (saving.value || mode.value === value) return;
  saving.value = true;
  try { await settings.set("browser.mode", value); error.value = null; if (value === "personal_chrome") await loadExtension(); }
  catch (e) { error.value = e instanceof Error ? e.message : String(e); }
  finally { saving.value = false; }
}
</script>

<template>
  <div class="flex h-full flex-col">
    <PageHeader title="Conectar aplicativos">
      <template #subtitle>Sistemas que os DOTs podem usar, como o Flexi. Os assistentes de IA ficam em <RouterLink to="/settings" class="text-violet-400 hover:text-violet-300">Configurações</RouterLink>.</template>
    </PageHeader>
    <div class="space-y-3 overflow-y-auto px-6 pb-8">
      <h2 class="font-mono text-[11px] font-semibold uppercase tracking-widest text-zinc-500">Sistemas</h2>
      <FlexiCard />
      <section class="card max-w-2xl space-y-3">
        <h3 class="text-sm font-semibold text-zinc-200">Navegador dos DOTs</h3>
        <p class="text-xs text-zinc-400">Escolha onde os DOTs abrem sites. As ferramentas e aprovacoes do DOTS continuam valendo para cada agente.</p>
        <ErrorBanner :message="error" />
        <label class="flex cursor-pointer items-start gap-3 text-sm text-zinc-200">
          <input type="radio" name="browser-mode" class="mt-1" :checked="mode === 'isolated'" :disabled="saving" @change="selectMode('isolated')" />
          <span><strong>Navegador isolado</strong><small class="block text-xs text-zinc-500">Cada DOT tem seus proprios cookies e logins.</small></span>
        </label>
        <label class="flex cursor-pointer items-start gap-3 text-sm text-zinc-200">
          <input type="radio" name="browser-mode" class="mt-1" :checked="mode === 'personal_chrome'" :disabled="saving" @change="selectMode('personal_chrome')" />
          <span><strong>Meu Chromium pessoal</strong><small class="block text-xs text-zinc-500">No Ubuntu, cada DOT usa uma aba no perfil do Chromium onde voce instalar a extensao.</small></span>
        </label>
        <div v-if="mode === 'personal_chrome'" class="space-y-2 rounded-lg border border-zinc-700 p-3 text-xs text-zinc-300">
          <p>1. No Chromium do perfil desejado, carregue a extensao sem compactacao desta pasta:</p>
          <code class="block break-all rounded bg-zinc-900 p-2 text-zinc-100">{{ extension?.path ?? 'Preparando extensao…' }}</code>
          <p>2. Copie o ID mostrado pelo Chromium e registre-o aqui para conectar a extensao ao DOTS:</p>
          <form class="flex gap-2" @submit.prevent="loadExtension(extensionId.trim())">
            <input v-model="extensionId" class="input min-w-0 flex-1" minlength="32" maxlength="32" pattern="[a-p]{32}" placeholder="ID da extensao (32 letras)" aria-label="ID da extensao Chromium" required />
            <button class="btn-primary shrink-0" type="submit">Conectar</button>
          </form>
          <p>{{ extension?.connected ? 'Extensao conectada' : 'Aguardando a extensao se conectar' }}. Depois abra o Computador de um DOT para acessar os sites e fazer login.</p>
          <button class="text-indigo-400 hover:underline" type="button" @click="loadExtension()">Atualizar estado</button>
        </div>
      </section>
    </div>
  </div>
</template>
