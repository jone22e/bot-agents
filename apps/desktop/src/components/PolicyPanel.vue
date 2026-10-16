<script setup lang="ts">
import { onMounted, reactive, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import type { AutoApproveLevel } from "@dots/shared-types";
import ErrorBanner from "@/components/ErrorBanner.vue";
import { TRUST_OPTIONS } from "@/lib/templates";
import { usePoliciesStore } from "@/stores/policies";

const props = defineProps<{ dotId: string }>();
const store = usePoliciesStore();
const form = reactive({ level: "read" as AutoApproveLevel, allowed_commands: "", allow_network: false, engine_native_write: false, max_file_mb: 2 });
const newWrite = ref(true);

onMounted(() => store.load(props.dotId));
watch(() => store.policy, (p) => {
  if (!p) return;
  form.level = p.auto_approve_level;
  form.allowed_commands = p.allowed_commands.join("\n");
  form.allow_network = p.allow_network;
  form.engine_native_write = p.engine_native_write;
  form.max_file_mb = Math.round((p.max_file_bytes / 1048576) * 10) / 10;
}, { immediate: true });

async function save() {
  await store.save({
    auto_approve_level: form.level,
    allowed_commands: form.allowed_commands.split("\n").map((s) => s.trim()).filter(Boolean),
    allow_network: form.allow_network,
    engine_native_write: form.engine_native_write,
    max_file_bytes: Math.max(1, Math.round(form.max_file_mb * 1048576)),
  });
}
async function pickDir() {
  const dir = await open({ directory: true, multiple: false, title: "Escolha a pasta que este DOT pode usar" });
  if (typeof dir === "string") await store.addWorkspace(dir, newWrite.value);
}
</script>

<template>
  <div class="max-w-2xl space-y-5">
    <ErrorBanner :message="store.error" />
    <section class="card space-y-3">
      <h3 class="text-sm font-semibold text-zinc-200">Quanto ele pode fazer sozinho</h3>
      <div class="space-y-2">
        <label v-for="o in TRUST_OPTIONS" :key="o.value" class="flex cursor-pointer items-start gap-3 rounded-md border border-zinc-800 p-3 hover:bg-zinc-900/60" :class="form.level === o.value ? 'border-indigo-600 bg-zinc-900/60' : ''">
          <input v-model="form.level" type="radio" :value="o.value" class="mt-1" />
          <span><span class="block text-sm text-zinc-100">{{ o.title }}</span><span class="block text-xs text-zinc-500">{{ o.description }}</span></span>
        </label>
        <label class="flex cursor-pointer items-start gap-3 rounded-md border border-zinc-800 p-3 hover:bg-zinc-900/60" :class="form.level === 'none' ? 'border-indigo-600 bg-zinc-900/60' : ''">
          <input v-model="form.level" type="radio" value="none" class="mt-1" />
          <span><span class="block text-sm text-zinc-100">Pergunta antes de tudo, ate para ler</span><span class="block text-xs text-zinc-500">Maximo de controle. Cada acao pede permissao.</span></span>
        </label>
      </div>
      <div v-if="form.level === 'exec'">
        <label class="label" for="cmds">Comandos que ele pode rodar sem perguntar (um por linha)</label>
        <textarea id="cmds" v-model="form.allowed_commands" class="input font-mono text-xs" rows="3" placeholder="npm test&#10;cargo check"></textarea>
        <p class="mt-1 text-xs text-zinc-500">Vale o comeco do comando: "npm test" libera "npm test --watch". Comandos perigosos (sudo, apagar tudo, baixar e executar) sao sempre bloqueados.</p>
      </div>
      <details class="text-xs">
        <summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Avancado</summary>
        <div class="mt-3 space-y-2 text-sm text-zinc-300">
          <label class="flex items-center gap-2"><input v-model="form.allow_network" type="checkbox" /> Permitir acesso a internet (instalar pacotes, git push)</label>
          <label class="flex items-center gap-2"><input v-model="form.engine_native_write" type="checkbox" /> Deixar o assistente usar as proprias ferramentas de edicao (fora do controle fino do DOTS)</label>
          <label class="flex items-center gap-2">Tamanho maximo de arquivo (MB) <input v-model.number="form.max_file_mb" type="number" min="0.1" step="0.1" class="input w-24" /></label>
        </div>
      </details>
      <div class="flex justify-end"><button class="btn-primary" :disabled="store.saving" @click="save">{{ store.saving ? "Salvando..." : "Salvar" }}</button></div>
    </section>

    <section class="card space-y-2">
      <h3 class="text-sm font-semibold text-zinc-200">Permissoes aprendidas</h3>
      <p class="text-xs text-zinc-500">Quando voce (ou o gerente) permite uma acao, ele aprende e nao pede de novo para o mesmo tipo de acao. Esqueca para ele voltar a pedir.</p>
      <ul v-if="store.learned.length" class="divide-y divide-zinc-800 text-sm">
        <li v-for="l in store.learned" :key="l.id" class="flex items-center justify-between gap-3 py-2">
          <div class="min-w-0">
            <span class="block truncate text-zinc-200">{{ l.summary || l.action_key }}</span>
            <span class="text-[11px] text-zinc-500">desde {{ new Date(l.created_at).toLocaleDateString() }}</span>
          </div>
          <button class="shrink-0 text-xs text-red-300 hover:underline" @click="store.forget(l.id)">esquecer</button>
        </li>
      </ul>
      <p v-else class="text-xs text-zinc-500">Nenhuma ainda.</p>
    </section>

    <section class="card">
      <div class="flex items-center justify-between">
        <div>
          <h3 class="text-sm font-semibold text-zinc-200">Pastas que ele pode usar</h3>
          <p class="text-xs text-zinc-500">Fora destas pastas o DOT nao le nem escreve nada.</p>
        </div>
        <div class="flex items-center gap-3">
          <label class="flex items-center gap-1 text-xs text-zinc-400"><input v-model="newWrite" type="checkbox" /> pode alterar</label>
          <button class="btn-secondary" @click="pickDir">Dar acesso a uma pasta...</button>
        </div>
      </div>
      <ul class="mt-3 divide-y divide-zinc-800 text-sm">
        <li v-for="w in store.workspaces" :key="w.id" class="flex items-center justify-between py-2">
          <div class="min-w-0">
            <span class="text-zinc-200">{{ w.path.split('/').filter(Boolean).pop() }}</span>
            <span class="ml-2 text-xs text-zinc-500">{{ w.is_primary ? "pasta de trabalho" : w.can_write ? "pode alterar" : "so leitura" }}</span>
            <p class="truncate font-mono text-[11px] text-zinc-600">{{ w.path }}</p>
          </div>
          <button v-if="!w.is_primary" class="text-xs text-red-300 hover:underline" @click="store.removeWorkspace(w.path)">remover acesso</button>
        </li>
      </ul>
    </section>
  </div>
</template>
