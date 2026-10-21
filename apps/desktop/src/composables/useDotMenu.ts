import { ref } from "vue";
import type { Dot } from "@dots/shared-types";

/** Menu de contexto (botao direito) de um DOT: um so aberto por vez, na posicao do clique. */
const current = ref<{ dot: Dot; x: number; y: number } | null>(null);

export function useDotMenu() {
  function open(e: MouseEvent, dot: Dot) {
    e.preventDefault();
    e.stopPropagation();
    current.value = { dot, x: e.clientX, y: e.clientY };
  }
  function close() { current.value = null; }
  return { current, open, close };
}
