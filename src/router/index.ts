import { createRouter, createWebHashHistory } from "vue-router";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      name: "dashboard",
      component: () => import("@views/Dashboard.vue"),
    },
    {
      path: "/files",
      name: "files",
      component: () => import("@views/Files.vue"),
    },
    {
      path: "/files/preview",
      name: "files-preview",
      component: () => import("@views/files/PreviewFiles.vue"),
    },
    {
      path: "/files/merge",
      name: "files-merge",
      component: () => import("@views/files/MergeFiles.vue"),
    },
    {
      path: "/files/translate",
      name: "files-translate",
      component: () => import("@views/files/TranslateFiles.vue"),
    },
    {
      path: "/files/combine",
      name: "files-combine",
      component: () => import("@views/files/CombineFiles.vue"),
    },
    {
      path: "/files/diff",
      name: "files-diff",
      component: () => import("@views/files/DiffFiles.vue"),
    },
    {
      path: "/files/dedupe",
      name: "files-dedupe",
      component: () => import("@views/files/DedupeFiles.vue"),
    },
    {
      path: "/files/transform",
      name: "files-transform",
      component: () => import("@views/files/TransformFiles.vue"),
    },
    {
      path: "/files/history",
      name: "files-history",
      component: () => import("@views/files/HistoryFiles.vue"),
    },
    {
      path: "/files/quality",
      name: "files-quality",
      component: () => import("@views/files/QualityCheck.vue"),
    },
    {
      path: "/files/cache",
      name: "files-cache",
      component: () => import("@views/files/CacheManage.vue"),
    },
    {
      path: "/settings",
      name: "settings",
      component: () => import("@views/Settings.vue"),
    },
  ],
});

export default router;
