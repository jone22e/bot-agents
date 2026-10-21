<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { Archive, Copy, MessageSquare, Pause, Play } from "lucide-vue-next";
import { useDotMenu } from "@/composables/useDotMenu";
import { useDotsStore } from "@/stores/dots";
import { useTeamsStore } from "@/stores/teams";

/**
 * Menu do botao direito sobre um DOT (lista de contatos e visao geral). Mesmas acoes rapidas do perfil:
 * pausar/retomar, duplicar e arquivar, mais abrir a conversa. Fecha com Esc, clique fora, rolagem ou ao sair da janela.
 */
const router = useRouter();
const menu = useDotMenu();
const dots = useDotsStore();
const teams = useTeamsStore();
const el = ref<HTMLElement | null>(null);
const pos = ref({ x: 0, y: 0 });
const dot = computed(() => menu.current.value?.dot ?? null);
const manages = computed(() => (dot.value ? teams.managedBy(dot.value.id) !== null : false));
/** Estado mais recente do DOT (o contato pode estar defasado ate o proximo recarregamento). */
const status = computed(() => (dot.value ? dots.byId(dot.value.id)?.status ?? dot.value.status : null));

watch(menu.current, async (m) => {
  if (!m) return;
  pos.value = { x: m.x, y: m.y };
  await nextTick();
  const r = el.value?.getBoundingClientRect();
  if (!r) return;
  const margin = 8;
  pos.value = {
    x: Math.max(margin, Math.min(m.x, window.innerWidth - r.width - margin)),
    y: Math.max(margin, Math.min(m.y, window.innerHeight - r.height - margin)),
  };
});

function onKey(e: KeyboardEvent) { if (e.key === "Escape") menu.close(); }
function onPointer(e: Event) { if (el.value && !el.value.contains(e.target as Node)) menu.close(); }
onMounted(() => {
  window.addEventListener("keydown", onKey);
  window.addEventListener("mousedown", onPointer, true);
  window.addEventListener("contextmenu", onPointer, true);
  window.addEventListener("scroll", menu.close, true);
  window.addEventListener("blur", menu.close);
  window.addEventListener("resize", menu.close);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  window.removeEventListener("mousedown", onPointer, true);
  window.removeEventListener("contextmenu", onPointer, true);
  window.removeEventListener("scroll", menu.close, true);
  window.removeEventListener("blur", menu.close);
  window.removeEventListener("resize", menu.close);
});

function run(action: (id: string) => Promise<unknown> | void) {
  const d = dot.value;
  menu.close();
  if (d) return action(d.id);
}
</script>

<template>
  <Teleport to="body">
    <div v-if="dot" ref="el" role="menu" :aria-label="`Acoes de ${dot.name}`" class="pop fixed z-50 w-56 rounded-xl border border-zinc-700 bg-zinc-900 py-1 text-sm shadow-2xl" :style="{ left: `${pos.x}px`, top: `${pos.y}px` }" @contextmenu.prevent>
      <p class="truncate px-3 py-1.5 text-xs font-medium text-zinc-400">{{ dot.name }}</p>
      <button role="menuitem" class="item" @click="run((id) => router.push(`/chat/${id}`))"><MessageSquare :size="15" /> Abrir conversa</button>
      <button v-if="status === 'active'" role="menuitem" class="item" data-action="pause" @click="run((id) => dots.pause(id))"><Pause :size="15" /> {{ manages ? "Pausar equipe" : "Pausar" }}</button>
      <button v-else role="menuitem" class="item" data-action="resume" @click="run((id) => dots.resume(id))"><Play :size="15" /> {{ manages ? "Retomar gerente" : "Retomar" }}</button>
      <button role="menuitem" class="item" @click="run((id) => dots.duplicate(id))"><Copy :size="15" /> Duplicar</button>
      <div class="my-1 border-t border-zinc-800"></div>
      <button role="menuitem" class="item" @click="run((id) => dots.archive(id))"><Archive :size="15" /> Arquivar</button>
    </div>
  </Teleport>
</template>

<style scoped>
.item { display: flex; width: 100%; align-items: center; gap: 0.5rem; padding: 0.5rem 0.75rem; text-align: left; color: var(--color-zinc-200); }
.item:hover { background: var(--color-zinc-800); color: var(--color-zinc-50); }
.item :deep(svg) { color: var(--color-zinc-400); }
.pop { animation: drop 0.12s ease-out; }
@keyframes drop { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: none; } }
@media (prefers-reduced-motion: reduce) { .pop { animation: none; } }
</style>
