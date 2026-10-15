import { createRouter, createWebHistory } from "vue-router";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", name: "home", component: () => import("@/views/ChatView.vue"), meta: { title: "DOTS" } },
    { path: "/chat/:dotId", name: "chat", component: () => import("@/views/ChatView.vue"), meta: { title: "Conversa" } },
    { path: "/dots/new", name: "dot-new", component: () => import("@/views/DotEditorView.vue"), meta: { title: "Novo DOT" } },
    { path: "/dots/:id/edit", name: "dot-edit", component: () => import("@/views/DotEditorView.vue"), meta: { title: "Editar DOT" } },
    { path: "/activities", name: "activities", component: () => import("@/views/ActivitiesView.vue"), meta: { title: "Atividades" } },
    { path: "/engines", name: "engines", component: () => import("@/views/EnginesView.vue"), meta: { title: "Conectar aplicativos" } },
    { path: "/teams", name: "teams", component: () => import("@/views/TeamsView.vue"), meta: { title: "Equipes" } },
    { path: "/settings", name: "settings", component: () => import("@/views/SettingsView.vue"), meta: { title: "Configuracoes" } },
    { path: "/onboarding", name: "onboarding", component: () => import("@/views/OnboardingView.vue"), meta: { title: "Primeiros passos", bare: true } },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
});
