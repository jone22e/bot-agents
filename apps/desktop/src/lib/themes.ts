/**
 * Temas (skins) do app, copiados do ovrgit. "dots" e o padrao (os cinzas do Tailwind em style.css); os demais
 * sobrescrevem as variaveis de cor: as do ovrgit (--accent, --bg, --panel...) e a escala `zinc`/`indigo`/... do
 * Tailwind v4 (cada utilitario `bg-zinc-900` vale `var(--color-zinc-900)`), de modo que a interface inteira muda.
 */

export interface ThemeColors {
  accent: string;
  accentStrong: string;
  onAccent: string;
  bg: string;
  panel: string;
  panel2: string;
  hover: string;
  border: string;
  text: string;
  muted: string;
  faint: string;
  add: string;
  del: string;
  mod: string;
  hunk: string;
}

export interface Theme {
  id: string;
  name: string;
  dark: boolean;
  colors?: ThemeColors;
}

export const DEFAULT_THEME = "dots";

export const THEMES: Theme[] = [
  { id: "dots", name: "DOTS (padrao)", dark: true },
  {
    id: "dracula",
    name: "Dracula",
    dark: true,
    colors: {
      accent: "#bd93f9", accentStrong: "#a87bf0", onAccent: "#282a36",
      bg: "#21222c", panel: "#282a36", panel2: "#313341", hover: "#3a3c4e", border: "#44475a",
      text: "#f8f8f2", muted: "#bfc2d9", faint: "#7b86b8",
      add: "#50fa7b", del: "#ff5555", mod: "#f1fa8c", hunk: "#8be9fd",
    },
  },
  {
    id: "one-dark",
    name: "One Dark Pro",
    dark: true,
    colors: {
      accent: "#61afef", accentStrong: "#4d9de0", onAccent: "#1b1d23",
      bg: "#21252b", panel: "#282c34", panel2: "#2c313a", hover: "#333842", border: "#3e4451",
      text: "#abb2bf", muted: "#8b93a1", faint: "#5c6370",
      add: "#98c379", del: "#e06c75", mod: "#e5c07b", hunk: "#c678dd",
    },
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    dark: true,
    colors: {
      accent: "#7aa2f7", accentStrong: "#5a86e8", onAccent: "#16161e",
      bg: "#16161e", panel: "#1a1b26", panel2: "#1f2335", hover: "#292e42", border: "#2a2f45",
      text: "#c0caf5", muted: "#a9b1d6", faint: "#737aa2",
      add: "#9ece6a", del: "#f7768e", mod: "#e0af68", hunk: "#bb9af7",
    },
  },
  {
    id: "monokai",
    name: "Monokai",
    dark: true,
    colors: {
      accent: "#f92672", accentStrong: "#e0135e", onAccent: "#ffffff",
      bg: "#1e1f1c", panel: "#272822", panel2: "#2f302a", hover: "#3e3d32", border: "#46473d",
      text: "#f8f8f2", muted: "#bebdb0", faint: "#8a8672",
      add: "#a6e22e", del: "#ff6188", mod: "#e6db74", hunk: "#66d9ef",
    },
  },
  {
    id: "nord",
    name: "Nord",
    dark: true,
    colors: {
      accent: "#88c0d0", accentStrong: "#81a1c1", onAccent: "#2e3440",
      bg: "#272c36", panel: "#2e3440", panel2: "#353c4a", hover: "#3b4252", border: "#434c5e",
      text: "#eceff4", muted: "#b4bccb", faint: "#7b88a1",
      add: "#a3be8c", del: "#bf616a", mod: "#ebcb8b", hunk: "#b48ead",
    },
  },
  {
    id: "github-dark",
    name: "GitHub Dark",
    dark: true,
    colors: {
      accent: "#4493f8", accentStrong: "#1f6feb", onAccent: "#ffffff",
      bg: "#010409", panel: "#0d1117", panel2: "#161b22", hover: "#1f252d", border: "#30363d",
      text: "#e6edf3", muted: "#9198a1", faint: "#6e7681",
      add: "#3fb950", del: "#f85149", mod: "#d29922", hunk: "#a371f7",
    },
  },
  {
    id: "github-light",
    name: "GitHub Light",
    dark: false,
    colors: {
      accent: "#0969da", accentStrong: "#0550ae", onAccent: "#ffffff",
      bg: "#f6f8fa", panel: "#ffffff", panel2: "#f6f8fa", hover: "#eaeef2", border: "#d0d7de",
      text: "#1f2328", muted: "#59636e", faint: "#818b98",
      add: "#1a7f37", del: "#cf222e", mod: "#9a6700", hunk: "#8250df",
    },
  },
  // Dust2: o mapa do Counter-Strike ao entardecer (arenito, caixotes, portas azuis do CT)
  {
    id: "cs-dust2",
    name: "Dust2",
    dark: true,
    colors: {
      accent: "#e3a545", accentStrong: "#c98a2b", onAccent: "#1f1a14",
      bg: "#1a150f", panel: "#221c15", panel2: "#2b241b", hover: "#372e22", border: "#463b2c",
      text: "#eadfc6", muted: "#b9aa8c", faint: "#8a7c63",
      add: "#9bbf5a", del: "#e0664d", mod: "#e08a3c", hunk: "#6fa8dc",
    },
  },
  // Mirage: a paleta do Ayu Mirage (azul-acinzentado escuro com destaque ambar)
  {
    id: "cs-mirage",
    name: "Mirage",
    dark: true,
    colors: {
      accent: "#ffcc66", accentStrong: "#ffad66", onAccent: "#1f2430",
      bg: "#1a1f29", panel: "#1f2430", panel2: "#242936", hover: "#2d3444", border: "#33394a",
      text: "#cccac2", muted: "#9ba3b2", faint: "#707a8c",
      add: "#87d96c", del: "#f27983", mod: "#ffad66", hunk: "#73d0ff",
    },
  },
  // Terminal: fundo quase preto e o verde dos terminais (xterm 77, sem neon); o resto em cinza, como um shell.
  // A interface vira mono e sem cantos arredondados (style.css, `[data-theme='terminal']`).
  {
    id: "terminal",
    name: "Terminal",
    dark: true,
    colors: {
      accent: "#5fd75f", accentStrong: "#4cbf4c", onAccent: "#061a0a",
      bg: "#000000", panel: "#070707", panel2: "#0f0f0f", hover: "#171717", border: "#222222",
      text: "#c9e8c9", muted: "#8fb58f", faint: "#5b7a5b",
      add: "#5fd75f", del: "#d75f5f", mod: "#d7af5f", hunk: "#5fafd7",
    },
  },
];

export const findTheme = (id: string | undefined | null) => THEMES.find((t) => t.id === id) ?? THEMES[0]!;

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}
const hex = (c: [number, number, number]) => "#" + c.map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");

/** Converte "#rrggbb" em "rgba(r, g, b, a)". */
export function alpha(h: string, a: number): string {
  const [r, g, b] = rgb(h);
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

/** Mistura `a` com `b` na proporcao `t` (0 = so a, 1 = so b). */
export function mix(a: string, b: string, t: number): string {
  const [r1, g1, b1] = rgb(a);
  const [r2, g2, b2] = rgb(b);
  return hex([r1 + (r2 - r1) * t, g1 + (g2 - g1) * t, b1 + (b2 - b1) * t]);
}

/**
 * Escala de uma cor de destaque (ambar, verde, vermelho, azul, acento) no formato do Tailwind: os tons claros
 * (50-300) puxam para o texto, o 500 e a cor em si, os escuros (600-950) puxam para o fundo.
 */
function scale(prefix: string, color: string, c: ThemeColors): Record<string, string> {
  const light = c.text;
  const dark = c.bg;
  return {
    [`--color-${prefix}-50`]: mix(color, light, 0.85),
    [`--color-${prefix}-100`]: mix(color, light, 0.7),
    [`--color-${prefix}-200`]: mix(color, light, 0.5),
    [`--color-${prefix}-300`]: mix(color, light, 0.3),
    [`--color-${prefix}-400`]: mix(color, light, 0.12),
    [`--color-${prefix}-500`]: color,
    [`--color-${prefix}-600`]: mix(color, dark, 0.12),
    [`--color-${prefix}-700`]: mix(color, dark, 0.3),
    [`--color-${prefix}-800`]: mix(color, dark, 0.5),
    [`--color-${prefix}-900`]: mix(color, dark, 0.7),
    [`--color-${prefix}-950`]: mix(color, dark, 0.85),
  };
}

/** Variaveis CSS do tema (vazio para o padrao, que usa style.css). */
export function themeVars(theme: Theme): Record<string, string> {
  const c = theme.colors;
  if (!c) return {};
  return {
    "--accent": c.accent,
    "--accent-strong": c.accentStrong,
    "--accent-soft": alpha(c.accent, theme.dark ? 0.18 : 0.12),
    "--on-accent": c.onAccent,
    "--bg": c.bg,
    "--panel": c.panel,
    "--panel-2": c.panel2,
    "--hover": c.hover,
    "--border": c.border,
    "--text": c.text,
    "--muted": c.muted,
    "--faint": c.faint,
    "--add": c.add,
    "--add-bg": alpha(c.add, theme.dark ? 0.14 : 0.12),
    "--del": c.del,
    "--del-bg": alpha(c.del, theme.dark ? 0.14 : 0.1),
    "--mod": c.mod,
    "--hunk": c.hunk,
    "--hunk-bg": alpha(c.hunk, theme.dark ? 0.1 : 0.08),
    "--sidebar": mix(c.bg, c.panel, 0.5),
    // escala neutra (zinc): do fundo (950) ao texto (50)
    "--color-zinc-950": c.bg,
    "--color-zinc-900": c.panel,
    "--color-zinc-800": c.panel2,
    "--color-zinc-700": c.hover,
    "--color-zinc-600": c.border,
    "--color-zinc-500": c.faint,
    "--color-zinc-400": c.muted,
    "--color-zinc-300": mix(c.muted, c.text, 0.5),
    "--color-zinc-200": mix(c.muted, c.text, 0.8),
    "--color-zinc-100": c.text,
    "--color-zinc-50": mix(c.text, theme.dark ? "#ffffff" : "#000000", 0.2),
    "--color-white": c.onAccent,
    ...scale("indigo", c.accent, c),
    "--color-indigo-600": c.accent,
    "--color-indigo-500": mix(c.accent, c.text, 0.1),
    "--color-indigo-700": c.accentStrong,
    ...scale("amber", c.mod, c),
    ...scale("emerald", c.add, c),
    ...scale("red", c.del, c),
    ...scale("sky", c.hunk, c),
  };
}
