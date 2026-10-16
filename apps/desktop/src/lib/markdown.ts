// Markdown das respostas do DOT -> HTML seguro. Copiado do ovrgit (AgentWindow.vue: md, withCopy).
import DOMPurify from "dompurify";
import { marked } from "marked";

/**
 * O resultado fica guardado por texto: a conversa se redesenha a cada pedaco de resposta enquanto o DOT trabalha,
 * e sem isso o markdown da conversa inteira era reprocessado toda vez. `stable: false` (resposta ainda chegando):
 * nao guarda, o texto muda a cada pedaco.
 */
const cache = new Map<string, string>();
const CACHE_MAX = 1500;

const SVG = (d: string) =>
  `<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="${d}"/></svg>`;
const COPY_BTN = `<button type="button" class="copy" title="Copiar">${SVG("M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2zM16 8V4a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h4")}${SVG("M20 6 9 17l-5-5")}</button>`;
/** Cada bloco de codigo ganha um botao de copiar no canto (o clique e tratado em `onMdClick`) */
const withCopy = (html: string) => html.replaceAll("<pre>", `<div class="code-wrap">${COPY_BTN}<pre>`).replaceAll("</pre>", "</pre></div>");

export function md(text: string, stable = true): string {
  if (stable) {
    const hit = cache.get(text);
    if (hit !== undefined) return hit;
  }
  const html = withCopy(DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true, breaks: false })));
  if (stable) {
    // o mais antigo sai quando enche (o Map guarda a ordem de insercao)
    if (cache.size >= CACHE_MAX) cache.delete(cache.keys().next().value as string);
    cache.set(text, html);
  }
  return html;
}

/** Clique dentro do markdown: copiar bloco de codigo; links abrem no navegador do sistema. */
export async function onMdClick(e: MouseEvent, openUrl: (url: string) => unknown) {
  const target = e.target as HTMLElement;
  const a = target.closest?.("a[href]") as HTMLAnchorElement | null;
  if (a) {
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) await openUrl(href);
    return;
  }
  const btn = target.closest?.(".code-wrap .copy") as HTMLButtonElement | null;
  if (!btn) return;
  const code = btn.parentElement?.querySelector("pre")?.textContent ?? "";
  try {
    await navigator.clipboard.writeText(code);
    btn.classList.add("done");
    setTimeout(() => btn.classList.remove("done"), 1500);
  } catch {
    /* sem acesso a area de transferencia */
  }
}
