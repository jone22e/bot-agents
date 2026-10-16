import { DEFAULT_THEME, findTheme, themeVars, type Theme } from "@/lib/themes";

/** Chave da configuracao (runtime) e espelho local para pintar antes de o servico responder. */
export const THEME_SETTING = "ui.theme";
const LOCAL_KEY = "dots.theme";

let applied: string[] = [];
let current: Theme = findTheme(DEFAULT_THEME);

export const currentTheme = () => current;

/** Aplica o tema: sobrescreve as variaveis CSS (o padrao volta ao style.css). */
export function applyTheme(id: string | undefined | null) {
  const theme = findTheme(id);
  current = theme;
  const root = document.documentElement;
  applied.forEach((k) => root.style.removeProperty(k));
  const vars = themeVars(theme);
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  applied = Object.keys(vars);
  root.dataset.theme = theme.id;
  root.style.colorScheme = theme.colors ? (theme.dark ? "dark" : "light") : "";
  try { localStorage.setItem(LOCAL_KEY, theme.id); } catch { /* sem localStorage */ }
}

/** Tema lembrado localmente (ultimo aplicado), para a primeira pintura. */
export function rememberedTheme(): string {
  try { return localStorage.getItem(LOCAL_KEY) ?? DEFAULT_THEME; } catch { return DEFAULT_THEME; }
}
