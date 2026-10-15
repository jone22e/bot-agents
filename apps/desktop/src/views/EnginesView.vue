<script setup lang="ts">
import PageHeader from "@/components/PageHeader.vue";
import FlexiCard from "@/components/FlexiCard.vue";
</script>

<template>
  <div class="flex h-full flex-col">
    <PageHeader title="Conectar aplicativos" subtitle="Sistemas que os DOTs podem usar, como o Flexi. Os assistentes de IA ficam em Configuracoes." />
    <div class="max-w-3xl space-y-4 overflow-y-auto px-6 pb-8">
    <h2 class="text-xs font-semibold uppercase tracking-wide text-zinc-500">Sistemas</h2>
    <FlexiCard />
    </div>
  </div>
</template>
