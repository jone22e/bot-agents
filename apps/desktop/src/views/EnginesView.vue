<script setup lang="ts">
import PageHeader from "@/components/PageHeader.vue";
import FlexiCard from "@/components/FlexiCard.vue";
</script>

<template>
  <div class="flex h-full flex-col">
    <PageHeader title="Conectar aplicativos">
      <template #subtitle>Sistemas que os DOTs podem usar, como o Flexi. Os assistentes de IA ficam em <RouterLink to="/settings" class="text-violet-400 hover:text-violet-300">Configurações</RouterLink>.</template>
    </PageHeader>
    <div class="space-y-3 overflow-y-auto px-6 pb-8">
      <h2 class="font-mono text-[11px] font-semibold uppercase tracking-widest text-zinc-500">Sistemas</h2>
      <FlexiCard />
    </div>
  </div>
</template>
