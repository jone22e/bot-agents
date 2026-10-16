import { beforeEach, describe, expect, it } from "vitest";
import { useColumnWidth } from "./useColumnWidth";

describe("useColumnWidth", () => {
  beforeEach(() => localStorage.clear());
  it("usa o padrao, limita ao intervalo e guarda", async () => {
    const c = useColumnWidth("teste", 300, 200, 500);
    expect(c.width.value).toBe(300);
    c.resize(400);
    expect(c.width.value).toBe(500);
    c.resize(-1000);
    expect(c.width.value).toBe(200);
    await Promise.resolve();
    expect(localStorage.getItem("dots.column.teste")).toBe("200");
    c.reset();
    expect(c.width.value).toBe(300);
  });
  it("le o valor guardado e ignora lixo", () => {
    localStorage.setItem("dots.column.a", "420");
    expect(useColumnWidth("a", 300, 200, 500).width.value).toBe(420);
    localStorage.setItem("dots.column.b", "abc");
    expect(useColumnWidth("b", 300, 200, 500).width.value).toBe(300);
    localStorage.setItem("dots.column.c", "9999");
    expect(useColumnWidth("c", 300, 200, 500).width.value).toBe(500);
  });
});
