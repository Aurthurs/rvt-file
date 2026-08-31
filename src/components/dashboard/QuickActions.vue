<script setup lang="ts">
import { computed, ref } from "vue";
import type { Component } from "vue";
import { useRouter } from "vue-router";
import { useMessage } from "naive-ui";
import {
  ArchiveOutline,
  CreateOutline,
  EyeOutline,
  GitMergeOutline,
  SettingsOutline,
  ShieldCheckmarkOutline,
} from "@vicons/ionicons5";
import { saveSettings, useSettings } from "@/composables/useSettings";

interface ActionItem {
  key: string;
  title: string;
  desc: string;
  icon: Component;
  to: string;
}

/** 可选的快捷入口池：全部已实现功能（扁平） */
const allActions: ActionItem[] = [
  {
    key: "preview",
    title: "预览文件",
    desc: "预览文件内容",
    icon: EyeOutline,
    to: "/files/preview",
  },
  {
    key: "merge",
    title: "文件融合",
    desc: "合并多个文件",
    icon: GitMergeOutline,
    to: "/files/merge",
  },
  {
    key: "quality",
    title: "质量检测",
    desc: "检查文件质量",
    icon: ShieldCheckmarkOutline,
    to: "/files/quality",
  },
  {
    key: "cache",
    title: "缓存管理",
    desc: "管理本地缓存",
    icon: ArchiveOutline,
    to: "/files/cache",
  },
  {
    key: "settings",
    title: "设置",
    desc: "应用偏好配置",
    icon: SettingsOutline,
    to: "/settings",
  },
];

/** 编辑弹窗的树形结构：模块分组 → 功能叶子 */
const treeData = [
  {
    label: "文件管理",
    key: "group-files",
    children: allActions
      .filter((a) => a.key !== "settings")
      .map((a) => ({ label: a.title, key: a.key })),
  },
  {
    label: "系统",
    key: "group-system",
    children: [{ label: "设置", key: "settings" }],
  },
];

const expandedKeys = ["group-files", "group-system"];

const router = useRouter();
const message = useMessage();
const { settings } = useSettings();

/** 展示的快捷入口：配置为空表示全部显示 */
const shownActions = computed(() => {
  const sel = settings.value.quick_entries;
  if (!sel.length) return allActions;
  return allActions.filter((a) => sel.includes(a.key));
});

// 编辑弹窗：树形勾选模块
const showModal = ref(false);
/** 弹窗内的勾选状态（仅叶子 key） */
const picking = ref<string[]>([]);

function openEdit() {
  const sel = settings.value.quick_entries;
  picking.value = sel.length ? [...sel] : allActions.map((a) => a.key);
  showModal.value = true;
}

function onUpdateChecked(keys: Array<string | number>) {
  picking.value = keys.map(String);
}

function pickAll() {
  picking.value = allActions.map((a) => a.key);
}

function pickNone() {
  picking.value = [];
}

async function confirmEdit() {
  if (!picking.value.length) {
    message.info("请至少保留一个快捷入口");
    return false; // 阻止弹窗关闭
  }
  settings.value.quick_entries = [...picking.value];
  await saveSettings();
  showModal.value = false;
  message.success("快捷入口已更新");
  return true;
}
</script>

<template>
  <section>
    <div class="head">
      <h2 class="section-title">快捷入口</h2>
      <n-button text size="small" :title="'编辑快捷入口'" @click="openEdit">
        <template #icon>
          <n-icon><CreateOutline /></n-icon>
        </template>
        编辑
      </n-button>
    </div>

    <div v-if="shownActions.length" class="grid">
      <button
        v-for="a in shownActions"
        :key="a.key"
        class="card"
        @click="router.push(a.to)"
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
    <n-empty
      v-else
      description="暂无快捷入口，点击右上角编辑添加"
      size="small"
    />

    <n-modal
      v-model:show="showModal"
      preset="dialog"
      title="编辑快捷入口"
      :positive-text="'保存'"
      negative-text="取消"
      style="width: 460px"
      @positive-click="confirmEdit"
    >
      <div class="modal-head">
        <span class="modal-tip">勾选要在工作台展示的模块</span>
        <div class="modal-actions">
          <n-button text size="small" @click="pickAll">全选</n-button>
          <n-button text size="small" @click="pickNone">清空</n-button>
        </div>
      </div>
      <n-tree
        block-line
        checkable
        check-strategy="child"
        :data="treeData"
        :checked-keys="picking"
        :default-expanded-keys="expandedKeys"
        @update:checked-keys="onUpdateChecked"
      />
    </n-modal>
  </section>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}

.section-title {
  font-size: 16px;
  font-weight: 600;
  margin: 0;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
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

/* 弹窗头 */
.modal-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}

.modal-tip {
  font-size: 13px;
  color: var(--text-muted);
}

.modal-actions {
  display: flex;
  gap: 10px;
}
</style>
