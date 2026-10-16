<script setup lang="ts">
import { onUnmounted, ref } from "vue";

/**
 * Alca vertical para ajustar a largura da coluna vizinha. `grows` diz para que lado o arrasto aumenta a coluna:
 * "right" (coluna a esquerda da alca, ex.: lista) ou "left" (coluna a direita, ex.: perfil).
 * `overlay`: a alca fica sobreposta a borda esquerda do proprio painel (para paineis com overflow-hidden), em vez de
 * ser um irmao de largura zero na linha flex. Emite `resize` com a variacao em pixels; duplo clique emite `reset`.
 */
const props = defineProps<{ grows: "left" | "right"; overlay?: boolean }>();
const emit = defineEmits<{ resize: [delta: number]; reset: [] }>();
const dragging = ref(false);
let lastX = 0;

function onMove(e: MouseEvent) {
  const delta = e.clientX - lastX;
  lastX = e.clientX;
  emit("resize", props.grows === "right" ? delta : -delta);
}
function stop() {
  dragging.value = false;
  document.body.style.cursor = "";
  document.body.style.userSelect = "";
  window.removeEventListener("mousemove", onMove);
  window.removeEventListener("mouseup", stop);
}
function start(e: MouseEvent) {
  e.preventDefault();
  dragging.value = true;
  lastX = e.clientX;
  document.body.style.cursor = "col-resize";
  document.body.style.userSelect = "none";
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", stop);
}
onUnmounted(stop);
</script>

<template>
  <div class="group z-10" :class="overlay ? 'absolute inset-y-0 left-0 w-0' : 'relative w-0 shrink-0 self-stretch'" title="Arraste para ajustar a largura; duplo clique volta ao padrao">
    <div class="absolute inset-y-0 w-[6px] cursor-col-resize" :class="overlay ? 'left-0' : '-left-[3px]'" @mousedown="start" @dblclick="emit('reset')">
      <div class="mx-auto h-full w-px transition-colors" :class="dragging ? 'bg-indigo-500' : 'bg-transparent group-hover:bg-indigo-500/60'"></div>
    </div>
  </div>
</template>
