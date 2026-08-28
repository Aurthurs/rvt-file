<script setup lang="ts">
import { useRouter } from "vue-router";
import {
  EyeOutline,
  GitMergeOutline,
  ShieldCheckmarkOutline,
} from "@vicons/ionicons5";

const router = useRouter();

const cards = [
  {
    title: "预览文件",
    desc: "快速预览文件内容",
    icon: EyeOutline,
    to: "/files/preview",
  },
  {
    title: "文件融合",
    desc: "合并多个文件",
    icon: GitMergeOutline,
    to: "/files/merge",
  },
  {
    title: "质量检测",
    desc: "检查文件完整性与质量",
    icon: ShieldCheckmarkOutline,
    to: "/files/quality",
  },
];
</script>

<template>
  <div class="files">
    <div class="page-head">
      <h2>文件管理</h2>
      <p>选择功能开始管理你的文件</p>
    </div>
    <div class="grid">
      <button
        v-for="c in cards"
        :key="c.title"
        class="card"
        @click="router.push(c.to)"
      >
        <div class="icon-wrap">
          <component :is="c.icon" />
        </div>
        <div class="meta">
          <span class="title">{{ c.title }}</span>
          <span class="desc">{{ c.desc }}</span>
        </div>
      </button>
    </div>
  </div>
</template>

<style scoped>
.files {
  max-width: 960px;
  margin: 0 auto;
}

.page-head {
  margin-bottom: 24px;
}

.page-head h2 {
  font-size: 22px;
  font-weight: 700;
  margin-bottom: 6px;
}

.page-head p {
  font-size: 13px;
  color: var(--text-muted);
}

.grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
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
