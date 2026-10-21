import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { moveId, useDragReorder } from "./useDragReorder";

describe("moveId", () => {
  it("coloca o item antes ou depois do alvo", () => {
    expect(moveId(["a", "b", "c"], "a", "c", true)).toEqual(["b", "a", "c"]);
    expect(moveId(["a", "b", "c"], "a", "c", false)).toEqual(["b", "c", "a"]);
    expect(moveId(["a", "b", "c"], "c", "a", true)).toEqual(["c", "a", "b"]);
  });
  it("devolve a mesma lista quando nao ha o que mover", () => {
    const ids = ["a", "b"];
    expect(moveId(ids, "a", "a", true)).toBe(ids);
    expect(moveId(ids, "a", "x", true)).toBe(ids);
    expect(moveId(ids, "x", "a", true)).toBe(ids);
  });
});

describe("useDragReorder", () => {
  const onDrop = vi.fn();
  let wrapper: ReturnType<typeof mount>;
  const pointer = (type: string, x: number, y: number, target: EventTarget = window) =>
    target.dispatchEvent(new MouseEvent(type, { clientX: x, clientY: y, button: 0, bubbles: true }));
  const rect = (top: number) => ({ top, bottom: top + 40, height: 40, left: 0, right: 100, width: 100, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;

  beforeEach(() => {
    onDrop.mockReset();
    const Comp = defineComponent({
      setup() {
        const drag = useDragReorder("y", onDrop);
        return () => ["a", "b"].map((id) => h("div", { "data-drag-id": id, id: `row-${id}`, class: drag.classOf(id), onPointerdown: (e: PointerEvent) => drag.start(e, id) }, id));
      },
    });
    wrapper = mount(Comp, { attachTo: document.body });
    const rows = { a: document.getElementById("row-a")!, b: document.getElementById("row-b")! };
    rows.a.getBoundingClientRect = () => rect(0);
    rows.b.getBoundingClientRect = () => rect(40);
    document.elementFromPoint = (_x: number, y: number) => (y < 40 ? rows.a : y < 80 ? rows.b : null);
  });
  afterEach(() => wrapper.unmount());

  it("so arrasta depois de alguns pixels e solta antes ou depois do alvo", async () => {
    const a = document.getElementById("row-a")!;
    pointer("pointerdown", 10, 10, a);
    pointer("pointermove", 11, 12);
    await wrapper.vm.$nextTick();
    expect(a.classList.contains("drag-source")).toBe(false);
    expect(document.body.classList.contains("drag-reordering")).toBe(false);

    pointer("pointermove", 10, 70);
    await wrapper.vm.$nextTick();
    expect(a.classList.contains("drag-source")).toBe(true);
    expect(document.body.classList.contains("drag-reordering")).toBe(true);
    expect(document.getElementById("row-b")!.classList.contains("drop-after")).toBe(true);

    pointer("pointerup", 10, 70);
    await wrapper.vm.$nextTick();
    expect(onDrop).toHaveBeenCalledWith("a", "b", false);
    expect(document.body.classList.contains("drag-reordering")).toBe(false);
    expect(a.classList.contains("drag-source")).toBe(false);
  });

  it("nao solta sobre si mesmo nem fora de um alvo, e ignora o botao direito", async () => {
    const a = document.getElementById("row-a")!;
    pointer("pointerdown", 10, 10, a);
    pointer("pointermove", 10, 30);
    pointer("pointerup", 10, 30);
    expect(onDrop).not.toHaveBeenCalled();

    pointer("pointerdown", 10, 10, a);
    pointer("pointermove", 10, 200);
    pointer("pointerup", 10, 200);
    expect(onDrop).not.toHaveBeenCalled();

    a.dispatchEvent(new MouseEvent("pointerdown", { clientX: 10, clientY: 10, button: 2, bubbles: true }));
    pointer("pointermove", 10, 70);
    pointer("pointerup", 10, 70);
    expect(onDrop).not.toHaveBeenCalled();
    expect(document.body.classList.contains("drag-reordering")).toBe(false);
  });
});
