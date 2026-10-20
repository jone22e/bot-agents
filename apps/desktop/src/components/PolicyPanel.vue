<script setup lang="ts">
import { onMounted, reactive, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronDown, FolderPlus } from "lucide-vue-next";
import type { AutoApproveLevel } from "@dots/shared-types";
import ErrorBanner from "@/components/ErrorBanner.vue";
import { usePoliciesStore } from "@/stores/policies";

const props = defineProps<{ dotId: string }>();
const store = usePoliciesStore();
const levels: { value: AutoApproveLevel; title: string; description: string }[] = [
  { value: "read", title: "Consultar arquivos", description: "Lê sem perguntar. Alterações e comandos pedem permissão." },
  { value: "write", title: "Editar arquivos", description: "Altera arquivos nas pastas liberadas. Comandos pedem permissão." },
  { value: "exec", title: "Editar e executar comandos", description: "Executa apenas os comandos liberados na lista abaixo." },
  { value: "none", title: "Pedir permissão para tudo", description: "Até a leitura depende de sua permissão." },
];
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
  <div class="max-w-2xl space-y-4">
    <ErrorBanner :message="store.error" />
    <section class="rounded-xl border border-zinc-800 bg-zinc-900/30 p-4">
      <h3 class="text-sm font-semibold text-zinc-100">Quanto ele pode fazer sozinho</h3>
      <p class="mt-1 text-xs leading-relaxed text-zinc-500">Escolha quando o DOT precisa pedir sua permissão.</p>
      <div class="mt-4 space-y-2">
        <label v-for="o in levels" :key="o.value" class="flex cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors" :class="form.level === o.value ? 'border-indigo-500/70 bg-indigo-500/10' : 'border-zinc-800 bg-zinc-950/20 hover:bg-zinc-800/50'">
          <input v-model="form.level" type="radio" :value="o.value" class="mt-0.5 shrink-0 accent-indigo-500" />
          <span class="min-w-0"><span class="block text-sm font-medium leading-snug text-zinc-100">{{ o.title }}</span><span class="mt-1 block text-xs leading-relaxed text-zinc-400">{{ o.description }}</span></span>
        </label>
      </div>
      <p class="mt-3 text-xs text-zinc-500">Ações irreversíveis sempre pedem permissão.</p>
      <div v-if="form.level === 'exec'" class="mt-4">
        <label class="label" for="cmds">Comandos que ele pode rodar sem perguntar (um por linha)</label>
        <textarea id="cmds" v-model="form.allowed_commands" class="input font-mono text-xs" rows="3" placeholder="npm test&#10;cargo check"></textarea>
        <p class="mt-1 text-xs text-zinc-500">Vale o comeco do comando: "npm test" libera "npm test --watch". Comandos perigosos (sudo, apagar tudo, baixar e executar) sao sempre bloqueados.</p>
      </div>
      <details class="mt-4 border-t border-zinc-800 text-xs">
        <summary class="flex cursor-pointer list-none items-center justify-between py-3 text-zinc-400 hover:text-zinc-200 [&::-webkit-details-marker]:hidden"><span>Avançado</span><ChevronDown :size="15" class="advanced-chevron transition-transform" /></summary>
        <div class="space-y-3 pb-2 text-sm text-zinc-300">
          <label class="flex items-start gap-2.5 leading-relaxed"><input v-model="form.allow_network" type="checkbox" class="mt-1 shrink-0 accent-indigo-500" /> Permitir acesso à internet (instalar pacotes, git push)</label>
          <label class="flex items-start gap-2.5 leading-relaxed"><input v-model="form.engine_native_write" type="checkbox" class="mt-1 shrink-0 accent-indigo-500" /> Deixar o assistente usar as próprias ferramentas de edição (fora do controle fino do DOTS)</label>
          <div><label class="label" for="max-file">Tamanho máximo de arquivo (MB)</label><input id="max-file" v-model.number="form.max_file_mb" type="number" min="0.1" step="0.1" class="input max-w-32" /></div>
        </div>
      </details>
      <div class="flex justify-end border-t border-zinc-800 pt-4"><button type="button" class="btn-primary" :disabled="store.saving" @click="save">{{ store.saving ? "Salvando..." : "Salvar" }}</button></div>
    </section>

    <section class="rounded-xl border border-zinc-800 bg-zinc-900/30 p-4">
      <h3 class="text-sm font-semibold text-zinc-100">Permissões aprendidas</h3>
      <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">Quando você ou o gerente autoriza uma ação, o DOT aprende a permissão. Esqueça uma permissão para ele voltar a pedir.</p>
      <ul v-if="store.learned.length" class="mt-3 divide-y divide-zinc-800 text-sm">
        <li v-for="l in store.learned" :key="l.id" class="flex items-center justify-between gap-3 py-2">
          <div class="min-w-0">
            <span class="block break-words text-zinc-200">{{ l.summary || l.action_key }}</span>
            <span class="text-[11px] text-zinc-500">desde {{ new Date(l.created_at).toLocaleDateString() }}</span>
          </div>
          <button type="button" class="shrink-0 text-xs text-red-300 hover:underline" @click="store.forget(l.id)">Esquecer</button>
        </li>
      </ul>
      <p v-else class="mt-3 text-xs text-zinc-500">Nenhuma permissão aprendida.</p>
    </section>

    <section class="rounded-xl border border-zinc-800 bg-zinc-900/30 p-4">
      <h3 class="text-sm font-semibold text-zinc-100">Pastas que ele pode usar</h3>
      <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">Fora destas pastas o DOT não lê nem escreve arquivos.</p>
      <div class="mt-4 rounded-lg border border-zinc-800 bg-zinc-950/20 p-3">
        <label class="flex items-center gap-2.5 text-xs text-zinc-300"><input v-model="newWrite" type="checkbox" class="shrink-0 accent-indigo-500" /> Permitir alterações na nova pasta</label>
        <button type="button" class="btn-secondary mt-3 w-full justify-center" @click="pickDir"><FolderPlus :size="15" /> Adicionar pasta</button>
      </div>
      <ul class="mt-3 divide-y divide-zinc-800 text-sm">
        <li v-for="w in store.workspaces" :key="w.id" class="flex items-start justify-between gap-3 py-3">
          <div class="min-w-0 flex-1">
            <span class="block break-words font-medium text-zinc-200">{{ w.is_primary ? "Pasta de trabalho" : w.path.split('/').filter(Boolean).pop() || w.path }}</span>
            <span class="mt-0.5 block text-xs text-zinc-500">{{ w.can_write ? "Leitura e escrita" : "Somente leitura" }}</span>
            <p class="mt-1 break-all font-mono text-[11px] leading-relaxed text-zinc-400" :title="w.path">{{ w.path }}</p>
          </div>
          <button v-if="!w.is_primary" type="button" class="shrink-0 text-xs text-red-300 hover:underline" @click="store.removeWorkspace(w.path)">Remover</button>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
details[open] .advanced-chevron { transform: rotate(180deg); }
</style>
