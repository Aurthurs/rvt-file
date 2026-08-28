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
import { ref, watch } from "vue";
import { darkTheme, dateZhCN, useOsTheme, zhCN } from "naive-ui";
import AppSidebar from "@components/layout/AppSidebar.vue";
import { darkThemeOverrides, lightThemeOverrides } from "@styles/theme";

const isDark = ref(useOsTheme().value === "dark");

watch(
  isDark,
  (v) => {
    document.documentElement.dataset.theme = v ? "dark" : "light";
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
