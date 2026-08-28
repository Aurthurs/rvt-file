<script setup lang="ts">
import { useMessage } from "naive-ui";
import {
  CloudDownloadOutline,
  CreateOutline,
  FolderOpenOutline,
  SettingsOutline,
} from "@vicons/ionicons5";

const message = useMessage();

const actions = [
  { title: "新建文件", desc: "创建一个新文档", icon: CreateOutline },
  { title: "打开文件", desc: "浏览并打开本地文件", icon: FolderOpenOutline },
  { title: "导入项目", desc: "从外部导入工程", icon: CloudDownloadOutline },
  { title: "设置", desc: "偏好与应用配置", icon: SettingsOutline },
];

function onClick(title: string) {
  message.info(`${title}开发中，敬请期待`);
}
</script>

<template>
  <section>
    <h2 class="section-title">快捷入口</h2>
    <div class="grid">
      <button
        v-for="a in actions"
        :key="a.title"
        class="card"
        @click="onClick(a.title)"
      >
        <div class="icon-wrap">
          <component :is="a.icon" />
        </div>
        <div class="meta">
          <span class="title">{{ a.title }}</span>
          <span class="desc">{{ a.desc }}</span>
        </div>
      </button>
    </div>
  </section>
</template>

<style scoped>
.section-title {
  font-size: 16px;
  font-weight: 600;
  margin-bottom: 14px;
}

.grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 16px;
}

.card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 14px;
  padding: 24px;
  border: none;
  border-radius: var(--radius-lg);
  background: var(--surface);
  cursor: pointer;
  text-align: left;
  box-shadow: var(--shadow-sm);
  transition:
    transform 0.15s ease,
    box-shadow 0.15s ease;
}

.card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

.icon-wrap {
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 22px;
  color: var(--brand);
  background: var(--brand-soft);
  border-radius: var(--radius-md);
}

.meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
}

.desc {
  font-size: 12px;
  color: var(--text-muted);
}
</style>
