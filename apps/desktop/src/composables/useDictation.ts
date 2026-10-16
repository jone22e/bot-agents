import { computed, onUnmounted, ref } from "vue";
import { mediaApi } from "@/api";

/**
 * Ditado no compositor: grava o microfone e devolve o texto transcrito (o audio e transcrito neste computador
 * e descartado). Um clique comeca, outro termina; `cancel` descarta a gravacao. Copiado do ovrgit (dictation.ts).
 */
const MAX_SECONDS = 10 * 60;
/** Taxa em que o audio vai para o transcritor (fala nao precisa de mais, e o arquivo fica pequeno) */
const RATE = 16000;
const clean = (e: unknown) => String((e as Error)?.message ?? e);

/** WAV PCM de 16 bits, mono, a partir das amostras (-1 a 1) */
export function encodeWav(samples: Float32Array, sampleRate: number): Uint8Array {
  const out = new DataView(new ArrayBuffer(44 + samples.length * 2));
  const ascii = (at: number, s: string) => [...s].forEach((c, i) => out.setUint8(at + i, c.charCodeAt(0)));
  ascii(0, "RIFF");
  out.setUint32(4, 36 + samples.length * 2, true);
  ascii(8, "WAVE");
  ascii(12, "fmt ");
  out.setUint32(16, 16, true);
  out.setUint16(20, 1, true); // PCM
  out.setUint16(22, 1, true); // mono
  out.setUint32(24, sampleRate, true);
  out.setUint32(28, sampleRate * 2, true);
  out.setUint16(32, 2, true);
  out.setUint16(34, 16, true);
  ascii(36, "data");
  out.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]!));
    out.setInt16(44 + i * 2, s < 0 ? s * 0x8000 : s * 0x7fff, true);
  }
  return new Uint8Array(out.buffer);
}

export function useDictation(onText: (text: string) => void) {
  const phase = ref<"idle" | "recording" | "transcribing">("idle");
  const seconds = ref(0);
  const error = ref<string | null>(null);
  const time = computed(() => `${Math.floor(seconds.value / 60)}:${String(seconds.value % 60).padStart(2, "0")}`);
  /** O webview tem gravacao de audio (o ditado some quando nao tem) */
  const supported = typeof navigator !== "undefined" && !!navigator.mediaDevices?.getUserMedia && typeof MediaRecorder !== "undefined";
  let stream: MediaStream | null = null;
  let recorder: MediaRecorder | null = null;
  let chunks: Blob[] = [];
  let timer: ReturnType<typeof setInterval> | undefined;
  let discard = false;

  function release() {
    clearInterval(timer);
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
    recorder = null;
  }

  /** O que foi gravado vira WAV mono de 16 kHz, que e o que o transcritor le */
  async function toWav(blob: Blob): Promise<Uint8Array> {
    const ctx = new AudioContext();
    try {
      const decoded = await ctx.decodeAudioData(await blob.arrayBuffer());
      const offline = new OfflineAudioContext(1, Math.max(1, Math.ceil(decoded.duration * RATE)), RATE);
      const src = offline.createBufferSource();
      src.buffer = decoded;
      src.connect(offline.destination);
      src.start();
      return encodeWav((await offline.startRendering()).getChannelData(0), RATE);
    } finally {
      ctx.close().catch(() => undefined);
    }
  }

  async function finish() {
    const blob = new Blob(chunks, { type: recorder?.mimeType || "audio/mp4" });
    chunks = [];
    release();
    if (discard || !blob.size) return void (phase.value = "idle");
    phase.value = "transcribing";
    try {
      const text = await mediaApi.transcribe(await toWav(blob));
      if (text) onText(text);
    } catch (e) {
      error.value = clean(e);
    } finally {
      phase.value = "idle";
    }
  }

  async function start() {
    error.value = null;
    discard = false;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      recorder = new MediaRecorder(stream);
      chunks = [];
      recorder.ondataavailable = (e) => e.data.size && chunks.push(e.data);
      recorder.onstop = finish;
      recorder.start();
      seconds.value = 0;
      phase.value = "recording";
      timer = setInterval(() => ++seconds.value >= MAX_SECONDS && stop(), 1000);
    } catch (e) {
      release();
      phase.value = "idle";
      const name = (e as Error)?.name;
      error.value = name === "NotAllowedError" ? "Sem acesso ao microfone. Libere em Ajustes do Sistema › Privacidade e Seguranca › Microfone." : clean(e);
    }
  }
  function stop() {
    if (recorder?.state === "recording") recorder.stop();
  }
  /** Um clique comeca a gravar; outro termina e transcreve */
  function toggle() {
    if (phase.value === "idle") start();
    else if (phase.value === "recording") stop();
  }
  /** Descarta a gravacao em andamento */
  function cancel() {
    if (phase.value !== "recording") return;
    discard = true;
    stop();
  }
  onUnmounted(() => {
    discard = true;
    stop();
    release();
  });
  return { phase, seconds, time, error, supported, toggle, cancel };
}
