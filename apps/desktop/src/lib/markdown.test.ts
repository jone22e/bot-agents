import { describe, expect, it } from "vitest";
import { md } from "./markdown";
import { encodeWav } from "@/composables/useDictation";

describe("markdown da resposta", () => {
  it("renders markdown, adds copy button to code blocks and strips scripts", () => {
    const html = md("**oi**\n\n```\nx = 1\n```\n\n<script>alert(1)</script>");
    expect(html).toContain("<strong>oi</strong>");
    expect(html).toContain('<div class="code-wrap"><button type="button" class="copy"');
    expect(html).not.toContain("<script");
  });
});

describe("encodeWav", () => {
  it("writes a 16-bit mono PCM header", () => {
    const wav = encodeWav(new Float32Array([0, 1, -1]), 16000);
    const v = new DataView(wav.buffer);
    expect(String.fromCharCode(...wav.slice(0, 4))).toBe("RIFF");
    expect(v.getUint16(22, true)).toBe(1);
    expect(v.getUint32(24, true)).toBe(16000);
    expect(v.getUint32(40, true)).toBe(6);
    expect(v.getInt16(46, true)).toBe(0x7fff);
    expect(v.getInt16(48, true)).toBe(-0x8000);
  });
});
