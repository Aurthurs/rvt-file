<template>
  <n-config-provider
    :locale="zhCN"
    :date-locale="dateZhCN"
    :theme="isDark ? darkTheme : null"
    :theme-overrides="isDark ? darkThemeOverrides : lightThemeOverrides"
  >
    <n-message-provider>
      <div class="layout">
        <AppSidebar v-model:dark="isDark" />
        <main class="content">
          <router-view />
        </main>
      </div>
    </n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, onMounted, watch } from "vue";
import { darkTheme, dateZhCN, zhCN } from "naive-ui";
import { invoke } from "@tauri-apps/api/core";
import AppSidebar from "@components/layout/AppSidebar.vue";
import { darkThemeOverrides, lightThemeOverrides } from "@styles/theme";
import { loadSettings, saveSettings, useSettings } from "@/composables/useSettings";

const { settings } = useSettings();

/** 主题唯一状态：写入 settings 并立即持久化 */
const isDark = computed({
  get: () => settings.value.theme === "dark",
  set: (v: boolean) => {
    settings.value.theme = v ? "dark" : "light";
    saveSettings();
  },
});

onMounted(loadSettings);

watch(
  () => settings.value.theme,
  (t) => {
    const v = t === "dark";
    document.documentElement.dataset.theme = v ? "dark" : "light";
    // 硬切 Windows 系统标题栏明暗（Tauri setTheme 在 Windows 上不可靠）
    invoke("set_window_theme", { req: { dark: v } }).catch(() => {});
  },
  { immediate: true }
);
</script>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
}

.content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 24px;
}
</style>
