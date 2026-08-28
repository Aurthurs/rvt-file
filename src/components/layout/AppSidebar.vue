<template>
  <aside class="sidebar">
    <div class="brand">
      <img src="/src/assets/avatar.png" class="avatar" />
    </div>

    <n-menu
      :options="menuOptions"
      :value="activeKey"
      collapsed
      :collapsed-width="64"
      :collapsed-icon-size="22"
      :on-update:value="(k) => onSelect(k as string)"
    />

    <div class="footer">
      <n-button
        quaternary
        circle
        size="small"
        :title="dark ? '切换到浅色模式' : '切换到暗色模式'"
        @click="emit('update:dark', !dark)"
      >
        <template #icon>
          <n-icon>
            <SunnyOutline v-if="dark" />
            <MoonOutline v-else />
          </n-icon>
        </template>
      </n-button>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { computed, h } from "vue";
import { NButton, NIcon, NMenu, useMessage } from "naive-ui";
import { useRoute, useRouter } from "vue-router";
import {
  FolderOpenOutline,
  HomeOutline,
  MoonOutline,
  SettingsOutline,
  SunnyOutline,
} from "@vicons/ionicons5";

defineProps<{ dark: boolean }>();
const emit = defineEmits<{ "update:dark": [v: boolean] }>();

const message = useMessage();
const route = useRoute();
const router = useRouter();

const menuOptions = [
  {
    label: "工作台",
    key: "dashboard",
    icon: () => h(NIcon, null, { default: () => h(HomeOutline) }),
  },
  {
    label: "文件管理",
    key: "files",
    icon: () => h(NIcon, null, { default: () => h(FolderOpenOutline) }),
  },
  {
    label: "设置",
    key: "settings",
    icon: () => h(NIcon, null, { default: () => h(SettingsOutline) }),
  },
];

const activeKey = computed(() => {
  const name = String(route.name ?? "");
  if (name.startsWith("files")) return "files";
  return name || "dashboard";
});

function onSelect(key: string) {
  if (key === "dashboard") router.push("/");
  else if (key === "files") router.push("/files");
  else message.info("设置页开发中");
}
</script>

<style scoped>
.sidebar {
  width: 64px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  padding: 16px 0;
  background: var(--surface);
  border-right: 1px solid var(--border);
}

.brand {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 8px 0;
  margin-bottom: 4px;
}

.avatar {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-sm);
}

.footer {
  margin-top: auto;
  display: flex;
  justify-content: center;
  padding-top: 12px;
}
</style>
