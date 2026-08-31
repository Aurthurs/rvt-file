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
