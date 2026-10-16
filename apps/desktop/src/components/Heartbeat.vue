<script setup lang="ts">
import { computed } from "vue";
import { HB_HEIGHT, HB_WIDTH, RHYTHMS, heartbeatPath, heartbeatSpan, type HeartbeatTone } from "@/lib/heartbeat";

/**
 * Sinais vitais (copiado do ovrgit): tracado de eletrocardiograma com um ritmo por estado (ver lib/heartbeat).
 * O <svg> inteiro rola dentro do involucro de largura fixa (transformacao composta na GPU); as bordas esmaecem
 * com uma mascara (e nao com degrades na cor do fundo: a linha selecionada ou sob o mouse tem outro fundo e os
 * degrades apareciam como bordas). Sao poucas linhas animadas, o custo da mascara nao pesa aqui.
 */
const props = defineProps<{ tone: HeartbeatTone }>();
const rhythm = computed(() => RHYTHMS[props.tone]);
const span = computed(() => heartbeatSpan(props.tone));
const d = computed(() => heartbeatPath(props.tone));
/* a animacao rola um compasso por ciclo: a distancia e a duracao vem do ritmo */
const style = computed(() => ({ "--hb-x": `${-rhythm.value.cell}px`, "--hb-s": `${rhythm.value.seconds}s` }));
</script>

<template>
  <span class="hb" :class="tone" :style="{ width: `${HB_WIDTH}px`, height: `${HB_HEIGHT}px` }" role="img" :aria-label="rhythm.label" :title="rhythm.label">
    <svg class="roll" :class="{ still: !rhythm.seconds }" :width="span" :height="HB_HEIGHT" :viewBox="`0 0 ${span} ${HB_HEIGHT}`" :style="style" aria-hidden="true">
      <path class="trace" :d="d" />
    </svg>
  </span>
</template>

<style scoped>
.hb { position: relative; display: block; flex: none; overflow: hidden; color: var(--faint); opacity: 0.45; }
.hb.live { color: var(--accent); opacity: 1; }
.hb.waiting { color: var(--mod); opacity: 1; }
.hb.done { color: var(--add); opacity: 0.85; }
.hb.stalled { color: var(--del); opacity: 1; }
.hb.error { color: var(--del); opacity: 0.55; }
/* bordas esmaecidas: mascara transparente nas pontas, independente do fundo da linha */
.hb { -webkit-mask-image: linear-gradient(90deg, transparent, #000 12px, #000 calc(100% - 12px), transparent); mask-image: linear-gradient(90deg, transparent, #000 12px, #000 calc(100% - 12px), transparent); }
.roll { display: block; animation: hb-scroll var(--hb-s) linear infinite; will-change: transform; }
.roll.still { animation: none; will-change: auto; }
.trace { fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linejoin: round; stroke-linecap: round; }
@keyframes hb-scroll { to { transform: translateX(var(--hb-x)); } }
@media (prefers-reduced-motion: reduce) { .roll { animation: none; will-change: auto; } }
</style>
