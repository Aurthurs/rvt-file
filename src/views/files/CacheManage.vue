<script setup lang="ts">
import { h, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { NButton, NDropdown, NPopconfirm, NTag, useMessage } from "naive-ui";
import { RefreshOutline } from "@vicons/ionicons5";
import BackBar from "@components/common/BackBar.vue";
import { useSettings } from "@/composables/useSettings";

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
const { settings } = useSettings();

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

/** 勾选的文件名 */
const checkedKeys = ref<string[]>([]);
/** 导出进行中（大文件导出时显示遮罩防止误操作） */
const exporting = ref(false);

const columns = [
  { type: "selection" as const },
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
    width: 120,
    render: (row: CacheEntry) =>
      h(
        "div",
        { style: "display:flex;gap:8px" },
        [
          h(
            NDropdown,
            {
              options: exportOptions,
              onSelect: (k: string) => onExportSingle(row, k),
            },
            {
              // n-dropdown 的触发元素是 default slot
              default: () =>
                h(
                  NButton,
                  { size: "small", secondary: true },
                  { default: () => "导出" }
                ),
            }
          ),
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
        ]
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
    // 同步勾选状态：移除列表中已不存在的项（例如刚被单独删除的文件）
    const valid = new Set(res.entries.map((e) => e.name));
    checkedKeys.value = checkedKeys.value.filter((k) => valid.has(k));
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

/** 批量删除勾选的文件 */
async function onBatchDelete() {
  const keys = [...checkedKeys.value];
  for (const name of keys) {
    await invoke("delete_cache", { name }).catch(() => {});
  }
  message.success(`已删除 ${keys.length} 项`);
  checkedKeys.value = [];
  refresh();
}

const exportOptions = [
  { label: "CSV", key: "csv" },
  { label: "Excel (.xlsx)", key: "xlsx" },
  { label: "Parquet", key: "parquet" },
];

/** 批量导出：每个选中文件独立导出为所选格式 */
async function onExport(format: string) {
  if (!checkedKeys.value.length) return;
  const dir = await open({ directory: true, title: "选择导出目录" });
  if (!dir) return;
  exporting.value = true;
  try {
    const res = await invoke<{ exported: string[]; total: number }>(
      "export_files",
      {
        req: {
          keys: [...checkedKeys.value],
          format,
          output_dir: dir,
          merge: false,
        },
      }
    );
    message.success(`已导出 ${res.total} 个文件到 ${dir}`);
  } catch (e) {
    message.error(String(e));
  } finally {
    exporting.value = false;
  }
}

/** 单个文件导出（行内操作，格式下拉与批量一致） */
async function onExportSingle(entry: CacheEntry, format: string) {
  const dir = await open({ directory: true, title: "选择导出目录" });
  if (!dir) return;
  exporting.value = true;
  try {
    await invoke<{ exported: string[]; total: number }>("export_files", {
      req: {
        keys: [entry.name],
        format,
        output_dir: dir,
        merge: false,
        file_name: null,
      },
    });
    message.success(`已导出 ${entry.name}`);
  } catch (e) {
    message.error(String(e));
  } finally {
    exporting.value = false;
  }
}

/** 合并导出：弹窗输入文件名，所有选中文件导出为一个 xlsx，每个文件一个 sheet */
const showNameModal = ref(false);
const exportName = ref("合并导出.xlsx");

function onExportMerge() {
  if (!checkedKeys.value.length) return;
  exportName.value = "合并导出.xlsx";
  showNameModal.value = true;
}

async function confirmExportMerge() {
  const name = exportName.value.trim();
  if (!name) {
    message.info("请输入文件名");
    return;
  }
  showNameModal.value = false;
  const dir = await open({ directory: true, title: "选择导出目录" });
  if (!dir) return;
  exporting.value = true;
  try {
    const res = await invoke<{ exported: string[]; total: number }>(
      "export_files",
      {
        req: {
          keys: [...checkedKeys.value],
          format: "xlsx",
          output_dir: dir,
          merge: true,
          file_name: name,
        },
      }
    );
    message.success(`已合并导出 ${res.total} 个文件到 ${dir}`);
  } catch (e) {
    message.error(String(e));
  } finally {
    exporting.value = false;
  }
}

onMounted(refresh);
</script>

<template>
  <div class="page">
    <BackBar />
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
        <n-popconfirm
          v-if="checkedKeys.length"
          @positive-click="onBatchDelete"
        >
          <template #trigger>
            <n-button type="error" secondary>
              批量删除 ({{ checkedKeys.length }})
            </n-button>
          </template>
          确认删除选中的 {{ checkedKeys.length }} 项？
        </n-popconfirm>
        <n-dropdown
          :options="exportOptions"
          :disabled="!checkedKeys.length"
          @select="(k) => onExport(k as string)"
        >
          <n-button :disabled="!checkedKeys.length">批量导出</n-button>
        </n-dropdown>
        <n-button :disabled="!checkedKeys.length" @click="onExportMerge">
          合并导出
        </n-button>
        <n-popconfirm @positive-click="onClear">
          <template #trigger>
            <n-button type="error" secondary :disabled="!entries.length">
              清空全部
            </n-button>
          </template>
          确认清空 data 目录下的所有缓存文件？此操作不可恢复。
        </n-popconfirm>
        <span class="path">{{ dataPath }}</span>
      </div>

      <n-data-table
        bordered
        :columns="columns"
        :data="entries"
        :row-key="(row: CacheEntry) => row.name"
        v-model:checked-row-keys="checkedKeys"
        :pagination="{ pageSize: settings.page_size }"
        :loading="loading"
      />
    </div>

    <n-modal
      v-model:show="showNameModal"
      preset="dialog"
      title="合并导出"
      :positive-text="'确定导出'"
      negative-text="取消"
      @positive-click="confirmExportMerge"
    >
      <n-input
        v-model:value="exportName"
        placeholder="输入导出文件名（自动补 .xlsx）"
        @keyup.enter="confirmExportMerge"
      />
    </n-modal>

    <div v-if="exporting" class="export-mask">
      <n-spin size="large" />
      <p class="export-text">正在导出，请稍候…</p>
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

/* 导出遮罩：大文件导出时阻止误操作 */
.export-mask {
  position: fixed;
  inset: 0;
  z-index: 2000;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  background: rgba(15, 23, 42, 0.45);
}

.export-text {
  color: #fff;
  font-size: 14px;
}
</style>
