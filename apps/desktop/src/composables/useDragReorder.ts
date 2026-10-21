import { onUnmounted, reactive } from "vue";

/** Nova lista com `from` colocado antes (ou depois) de `to`. Sem mudanca se `to` nao existir ou for o proprio `from`. */
export function moveId(ids: string[], from: string, to: string, before: boolean): string[] {
  if (from === to) return ids;
  const rest = ids.filter((x) => x !== from);
  const i = rest.indexOf(to);
  if (i < 0 || !ids.includes(from)) return ids;
  rest.splice(before ? i : i + 1, 0, from);
  return rest;
}

const ATTR = "data-drag-id";
const THRESHOLD = 5;

/**
 * Reordenacao por arrasto com eventos de ponteiro (o arrastar nativo do HTML nao funciona na janela do Tauri,
 * que captura o arrasto para receber arquivos). Cada item alvo leva `data-drag-id`; a alca chama `start` no
 * `pointerdown`. O arrasto so comeca depois de alguns pixels, entao clique e botao direito seguem normais.
 */
export function useDragReorder(axis: "x" | "y", onDrop: (from: string, to: string, before: boolean) => void) {
  const state = reactive({ dragging: null as string | null, over: null as string | null, before: true });
  let pressed: { id: string; x: number; y: number } | null = null;

  function start(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    pressed = { id, x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", cancel);
  }
  function move(e: PointerEvent) {
    if (!pressed) return;
    if (!state.dragging) {
      if (Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < THRESHOLD) return;
      state.dragging = pressed.id;
      document.body.classList.add("drag-reordering");
    }
    e.preventDefault();
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>(`[${ATTR}]`) ?? null;
    const id = el?.getAttribute(ATTR) ?? null;
    if (!el || !id || id === state.dragging) { state.over = null; return; }
    const r = el.getBoundingClientRect();
    state.over = id;
    state.before = axis === "y" ? e.clientY < r.top + r.height / 2 : e.clientX < r.left + r.width / 2;
  }
  function end() {
    const { dragging, over, before } = state;
    if (dragging) swallowNextClick();
    cancel();
    if (dragging && over) onDrop(dragging, over, before);
  }
  function cancel() {
    pressed = null;
    state.dragging = null;
    state.over = null;
    document.body.classList.remove("drag-reordering");
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", cancel);
  }
  /** O clique que o navegador dispara logo apos soltar nao deve abrir nada. */
  function swallowNextClick() {
    const eat = (ev: Event) => { ev.stopPropagation(); ev.preventDefault(); };
    window.addEventListener("click", eat, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", eat, { capture: true }), 0);
  }
  /** Classe do item: alvo antes/depois ou o proprio arrastado. */
  function classOf(id: string) {
    return { "drag-source": state.dragging === id, "drop-before": state.over === id && state.before, "drop-after": state.over === id && !state.before };
  }
  onUnmounted(cancel);
  return { state, start, classOf };
}
