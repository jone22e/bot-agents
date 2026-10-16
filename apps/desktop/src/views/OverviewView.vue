<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import { Check } from "lucide-vue-next";
import type { Contact } from "@dots/shared-types";
import { useContactsStore } from "@/stores/contacts";
import { useTeamsStore } from "@/stores/teams";

/**
 * Visao geral: uma coluna por equipe (gerente no topo, funcionarios embaixo) com a situacao de cada DOT.
 * Quem gerencia uma equipe aparece so na coluna dela, como na lista de contatos.
 */
const router = useRouter();
const contacts = useContactsStore();
const teams = useTeamsStore();

type Status = "working" | "done" | "waiting" | "idle";
const LEGEND: { key: Status; label: string }[] = [
  { key: "working", label: "Trabalhando" },
  { key: "done", label: "Concluído" },
  { key: "waiting", label: "Aguardando você" },
  { key: "idle", label: "Ocioso" },
];
/** attention = pede algo a voce; busy = trabalhando, na fila ou esperando o gerente; done = ultimo trabalho deu certo. */
const statusOf = (c: Contact): Status => (c.state === "attention" ? "waiting" : c.state === "busy" ? "working" : c.done ? "done" : "idle");
const label = (s: Status) => LEGEND.find((l) => l.key === s)!.label;

const columns = computed(() => {
  const byId = new Map(contacts.items.map((c) => [c.dot.id, c]));
  const managers = new Set(teams.items.map((t) => t.manager_dot_id).filter((x): x is string => !!x));
  const placed = new Set<string>();
  const out: { key: string; title: string; manager: Contact | null; members: Contact[] }[] = [];
  for (const t of teams.items) {
    const manager = t.manager_dot_id ? byId.get(t.manager_dot_id) ?? null : null;
    if (manager) placed.add(manager.dot.id);
    const members = t.member_ids.map((id) => byId.get(id)).filter((c): c is Contact => !!c && !managers.has(c.dot.id) && !placed.has(c.dot.id));
    members.forEach((c) => placed.add(c.dot.id));
    if (manager || members.length) out.push({ key: t.id, title: t.name, manager, members });
  }
  const rest = contacts.items.filter((c) => !placed.has(c.dot.id));
  if (rest.length) out.push({ key: "none", title: "Sem equipe", manager: null, members: rest });
  return out;
});
const open = (c: Contact) => router.push(`/chat/${c.dot.id}`);
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden">
    <header class="flex shrink-0 items-center justify-between gap-4 px-6 pb-4 pt-5">
      <h1 class="text-lg font-semibold text-zinc-100">Visão Geral</h1>
      <div class="flex flex-wrap items-center gap-4 text-xs text-zinc-400">
        <span v-for="l in LEGEND" :key="l.key" class="flex items-center gap-1.5"><span class="legend" :class="l.key"><Check v-if="l.key === 'done'" :size="8" :stroke-width="4" /></span>{{ l.label }}</span>
      </div>
    </header>
    <div class="min-h-0 flex-1 overflow-auto px-6 pb-6">
      <p v-if="!columns.length" class="text-sm text-zinc-500">Nenhum DOT ainda.</p>
      <div v-else class="flex min-w-full w-max rounded-2xl border border-zinc-800 bg-zinc-900/60">
        <section v-for="col in columns" :key="col.key" class="flex w-60 flex-1 flex-col border-r border-zinc-800 last:border-r-0">
          <div class="flex h-[72px] items-center border-b border-zinc-800 px-5">
            <button v-if="col.manager" class="row" :title="`${col.manager.dot.name} · ${label(statusOf(col.manager))}`" @click="open(col.manager)">
              <span class="ball" :class="statusOf(col.manager)"><Check v-if="statusOf(col.manager) === 'done'" :size="14" :stroke-width="3" /></span>
              <span class="min-w-0 text-left">
                <span class="block truncate text-sm font-semibold text-zinc-100">{{ col.manager.dot.name }}</span>
                <span class="block truncate text-[10px] uppercase tracking-[0.16em] text-zinc-500">{{ col.title }}</span>
              </span>
            </button>
            <span v-else class="truncate text-[10px] uppercase tracking-[0.16em] text-zinc-500">{{ col.title }}</span>
          </div>
          <div class="flex flex-col gap-1 px-5 py-3">
            <button v-for="c in col.members" :key="c.dot.id" class="row py-2" :title="`${c.dot.name} · ${label(statusOf(c))}`" @click="open(c)">
              <span class="ball" :class="statusOf(c)"><Check v-if="statusOf(c) === 'done'" :size="14" :stroke-width="3" /></span>
              <span class="truncate text-sm text-zinc-200">{{ c.dot.name }}</span>
            </button>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.row { display: flex; width: 100%; min-width: 0; align-items: center; gap: 0.75rem; border-radius: 0.75rem; text-align: left; }
.row:hover .ball { filter: brightness(1.15); }
.ball { position: relative; display: inline-flex; height: 2rem; width: 2rem; flex-shrink: 0; align-items: center; justify-content: center; border-radius: 9999px; color: white; }
.ball.working { background: #8b5cf6; }
.ball.working::after { content: ""; position: absolute; inset: -4px; border-radius: 9999px; border: 3px solid rgb(139 92 246 / 0.25); border-top-color: rgb(139 92 246 / 0.8); animation: spin 1.1s linear infinite; }
.ball.done { background: #22c55e; }
.ball.waiting { background: #f97316; }
.ball.idle { background: #3f3f46; }
.legend { display: inline-flex; height: 0.75rem; width: 0.75rem; align-items: center; justify-content: center; border-radius: 9999px; color: white; }
.legend.working { background: #8b5cf6; box-shadow: 0 0 0 2px rgb(139 92 246 / 0.35); }
.legend.done { background: #22c55e; }
.legend.waiting { background: #f97316; }
.legend.idle { background: #3f3f46; }
@keyframes spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .ball.working::after { animation: none; } }
</style>
