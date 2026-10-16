import { ref, watch, type Ref } from "vue";

/**
 * Largura de uma coluna (lista, perfil, computador) ajustavel pelo usuario e guardada no localStorage.
 * `min`/`max` limitam o arrasto; o valor volta ao padrao se o guardado for invalido.
 */
export function useColumnWidth(key: string, fallback: number, min: number, max: number): { width: Ref<number>; resize: (delta: number) => void; reset: () => void } {
  const storageKey = `dots.column.${key}`;
  const clamp = (v: number) => Math.round(Math.min(max, Math.max(min, v)));
  const stored = Number(typeof localStorage !== "undefined" ? localStorage.getItem(storageKey) : NaN);
  const width = ref(Number.isFinite(stored) && stored > 0 ? clamp(stored) : fallback);
  watch(width, (w) => { try { localStorage.setItem(storageKey, String(w)); } catch { /* sem armazenamento: so nesta sessao */ } });
  return {
    width,
    resize: (delta: number) => { width.value = clamp(width.value + delta); },
    reset: () => { width.value = fallback; },
  };
}
