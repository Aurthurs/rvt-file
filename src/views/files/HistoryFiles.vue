<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { useMessage } from "naive-ui";
import BackBar from "@components/common/BackBar.vue";
import { actionLabels } from "@/composables/useHistory";

interface HistoryEntry {
  time: number;
  action: string;
  title: string;
  detail: string;
}

const message = useMessage();
const router = useRouter();
const items = ref<HistoryEntry[]>([]);

async function load() {
  try {
    items.value = await invoke<HistoryEntry[]>("list_history");
  } catch (e) {
    message.error(String(e));
  }
}

onMounted(load);

function go(e: HistoryEntry) {
  router.push(e.detail);
}

async function onClear() {
  try {
    await invoke("clear_history");
    items.value = [];
    message.success("历史已清空");
  } catch (err) {
    message.error(String(err));
  }
}

function fmtTime(ms: number) {
  return new Date(ms).toLocaleString();
}
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>最近任务</h2>
    <p class="sub">最近的导入、导出与处理记录，点击条目回到对应现场</p>

    <div class="card">
      <div class="section-title">
        操作记录（{{ items.length }}）
        <n-button
          v-if="items.length"
          text
          type="error"
          size="small"
          @click="onClear"
        >
          清空
        </n-button>
      </div>

      <div v-if="items.length" class="list">
        <button
          v-for="(e, i) in items"
          :key="i"
          class="item"
          @click="go(e)"
        >
          <n-tag class="item-tag" size="small" :bordered="false">
            {{ actionLabels[e.action] || e.action }}
          </n-tag>
          <span class="item-title">{{ e.title }}</span>
          <span class="item-time">{{ fmtTime(e.time) }}</span>
        </button>
      </div>
      <n-empty
        v-else
        description="暂无记录，完成导入或处理后会出现在这里"
        size="small"
      />
    </div>
  </div>
</template>

<style scoped>
.page h2 {
  font-size: 22px;
  font-weight: 700;
  margin-bottom: 6px;
}

.sub {
  font-size: 13px;
  color: var(--text-muted);
}

.card {
  margin-top: 16px;
  padding: 16px 20px;
  background: var(--surface);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
}

.section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 14px;
  font-weight: 600;
  margin-bottom: 12px;
}

.list {
  display: flex;
  flex-direction: column;
}

.item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 10px 8px;
  border: none;
  border-bottom: 1px solid var(--border);
  background: transparent;
  cursor: pointer;
  text-align: left;
  border-radius: var(--radius-sm);
  transition: background 0.12s ease;
}

.item:last-child {
  border-bottom: none;
}

.item:hover {
  background: var(--brand-soft);
}

.item-tag {
  flex-shrink: 0;
  width: 52px;
  justify-content: center;
}

.item-title {
  flex: 1;
  font-size: 13px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.item-time {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--text-muted);
}
</style>
