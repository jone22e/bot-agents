/**
 * Sinais vitais do DOT (copiado do ovrgit): o tracado de eletrocardiograma na lista de contatos diz o estado
 * antes de ler qualquer texto. Cada estado tem um ritmo: trabalhando, firme e regular (pouco menos de 2 batidas
 * por segundo); esperando alguem (voce ou o gerente), lento e espacado (uma a cada ~2,6 s); concluido, em
 * repouso (batida pequena a cada ~5,5 s); parado (rodando sem saida), fora de compasso; falhou, linha reta.
 * O tracado e um caminho SVG que rola para a esquerda: `cell` e a largura de um compasso e `seconds` quanto
 * ele leva para passar.
 *
 * Custo: o <svg> inteiro e que se move (transformacao composta na GPU, sem repintar), dentro de um involucro de
 * largura fixa que o recorta; as bordas esmaecem com um degrade sobreposto, nao com mascara.
 */
export type HeartbeatTone = "live" | "waiting" | "done" | "stalled" | "error" | "idle" | "standby";

/** Largura fixa do tracado (px, igual para todos os estados) e linha de base */
export const HB_WIDTH = 60;
export const HB_HEIGHT = 16;
const BASE = 8;
/** Largura de uma batida */
export const BEAT_WIDTH = 24;

const n = (v: number) => String(+v.toFixed(2));
/** Uma batida (ondas P, QRS e T) de amplitude `amp`, em movimentos relativos, comecando e terminando na linha de base */
export const beat = (amp: number) =>
  [
    "l3,0", `l2,${n(-0.15 * amp)}`, `l2,${n(0.15 * amp)}`, "l2,0",
    `l1.5,${n(0.25 * amp)}`, `l2,${n(-1.25 * amp)}`, `l2,${n(1.45 * amp)}`, `l1.5,${n(-0.45 * amp)}`,
    "l3,0", `l2,${n(-0.3 * amp)}`, `l3,${n(0.3 * amp)}`,
  ].join("");

export interface Rhythm {
  /** Batidas dentro de um compasso: onde comeca cada uma (x) e a amplitude */
  beats: { x: number; amp: number }[];
  /** Largura do compasso */
  cell: number;
  /** Segundos que um compasso leva para passar (0: nao se move) */
  seconds: number;
  /** Dica ao passar o mouse */
  label: string;
}
export const RHYTHMS: Record<HeartbeatTone, Rhythm> = {
  live: { beats: [{ x: 4, amp: 6 }], cell: 32, seconds: 0.6, label: "Trabalhando: firme e regular" },
  waiting: { beats: [{ x: 48, amp: 6 }], cell: 120, seconds: 2.6, label: "Esperando resposta: lento e espacado" },
  done: { beats: [{ x: 48, amp: 2.2 }], cell: 120, seconds: 5.5, label: "Concluido: em repouso" },
  stalled: { beats: [{ x: 4, amp: 6 }, { x: 32, amp: 4 }, { x: 78, amp: 7 }], cell: 120, seconds: 1.6, label: "Parado: fora de compasso, sem avancar" },
  error: { beats: [], cell: 120, seconds: 0, label: "O ultimo trabalho nao deu certo: sem batida" },
  idle: { beats: [], cell: 120, seconds: 0, label: "" },
  standby: { beats: [], cell: 120, seconds: 0, label: "Na fila, em espera" },
};

/** Compassos necessarios: cobrem a janela mais um (o que sai pela esquerda ja entrou pela direita) */
const cellsOf = (tone: HeartbeatTone) => Math.ceil(HB_WIDTH / RHYTHMS[tone].cell) + 1;
/** Largura total do caminho (o <svg> que rola) */
export const heartbeatSpan = (tone: HeartbeatTone) => cellsOf(tone) * RHYTHMS[tone].cell;

/** Caminho do tracado: os compassos repetidos, de ponta a ponta */
export function heartbeatPath(tone: HeartbeatTone): string {
  const r = RHYTHMS[tone];
  const cells = cellsOf(tone);
  let d = `M0,${BASE}`;
  for (let c = 0; c < cells; c++) {
    let x = 0;
    for (const b of r.beats) {
      if (b.x > x) d += `h${n(b.x - x)}`;
      d += beat(b.amp);
      x = b.x + BEAT_WIDTH;
    }
    if (r.cell > x) d += `h${n(r.cell - x)}`;
  }
  return d;
}

/** Ritmo a partir do `pulse` que o runtime calcula por contato (`contacts.list`). */
export function toneOf(pulse: string | undefined | null): HeartbeatTone {
  switch (pulse) {
    case "live": case "waiting": case "done": case "stalled": case "error": case "standby": return pulse;
    default: return "idle";
  }
}
