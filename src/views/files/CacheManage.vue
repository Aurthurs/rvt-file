<script setup lang="ts">
import { h, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { NButton, NPopconfirm, NTag, useMessage } from "naive-ui";
import { RefreshOutline } from "@vicons/ionicons5";

interface CacheEntry {
  name: string;
  kind: "file" | "dir";
  size: number;
  modified: number;
}

const message = useMessage();
const loading = ref(false);
const dataPath = ref("");
const entries = ref<CacheEntry[]>([]);

function formatSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatTime(ts: number) {
  if (!ts) return "-";
  return new Date(ts * 1000).toLocaleString("zh-CN");
}

const columns = [
  { title: "名称", key: "name", ellipsis: { tooltip: true } },
  {
    title: "类型",
    key: "kind",
    width: 90,
    render: (row: CacheEntry) =>
      row.kind === "dir"
        ? h(NTag, { type: "success", size: "small" }, { default: () => "文件夹" })
        : h(NTag, { type: "info", size: "small" }, { default: () => "文件" }),
  },
  {
    title: "大小",
    key: "size",
    width: 110,
    render: (row: CacheEntry) => formatSize(row.size),
  },
  {
    title: "修改时间",
    key: "modified",
    width: 180,
    render: (row: CacheEntry) => formatTime(row.modified),
  },
  {
    title: "操作",
    key: "action",
    width: 90,
    render: (row: CacheEntry) =>
      h(
        NPopconfirm,
        { onPositiveClick: () => onDelete(row) },
        {
          trigger: () =>
            h(
              NButton,
              { size: "small", type: "error", quaternary: true },
              { default: () => "删除" }
            ),
          default: () => `确认删除 ${row.name}？`,
        }
      ),
  },
];

async function refresh() {
  loading.value = true;
  try {
    const res = await invoke<{ path: string; entries: CacheEntry[] }>(
      "list_cache"
    );
    dataPath.value = res.path;
    entries.value = res.entries;
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

async function onDelete(entry: CacheEntry) {
  try {
    await invoke("delete_cache", { name: entry.name });
    message.success(`已删除 ${entry.name}`);
    refresh();
  } catch (e) {
    message.error(String(e));
  }
}

async function onClear() {
  for (const e of entries.value) {
    await invoke("delete_cache", { name: e.name }).catch(() => {});
  }
  message.success("已清空缓存");
  refresh();
}

onMounted(refresh);
</script>

<template>
  <div class="page">
    <h2>缓存管理</h2>
    <p class="sub">管理 data 目录下的缓存文件与文件夹。</p>

    <div class="card">
      <div class="toolbar">
        <n-button secondary :loading="loading" @click="refresh">
          <template #icon>
            <n-icon><RefreshOutline /></n-icon>
          </template>
          刷新
        </n-button>
        <n-button type="error" secondary :disabled="!entries.length" @click="onClear">
          清空全部
        </n-button>
        <span class="path">{{ dataPath }}</span>
      </div>

      <n-data-table
        bordered
        :columns="columns"
        :data="entries"
        :row-key="(row: CacheEntry) => row.name"
        :pagination="{ pageSize: 10 }"
        :loading="loading"
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
  margin-top: 20px;
  padding: 20px;
  background: var(--surface);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}

.path {
  font-size: 12px;
  color: var(--text-muted);
  margin-left: auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 420px;
}
</style>
