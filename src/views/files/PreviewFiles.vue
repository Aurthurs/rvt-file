<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useMessage } from "naive-ui";
import { FilterOutline } from "@vicons/ionicons5";
import BackBar from "@components/common/BackBar.vue";
import { useSettings } from "@/composables/useSettings";

interface ImportedFile {
  key: string;
  source: string;
  sheet: string;
  columns: string[];
  rows: (string | number | null)[][];
  row_count: number;
}

interface SheetItem {
  key: string;
  sheet: string;
}

interface FileGroup {
  source: string;
  sheets: SheetItem[];
}

const message = useMessage();
const loading = ref(false);
/** 文件组：按源文件分组，每组含多个 sheet 的 parquet */
const groups = ref<FileGroup[]>([]);
const selectedSource = ref("");
const selectedSheet = ref("");
const columns = ref<string[]>([]);
const rows = ref<(string | number | null)[][]>([]);
const total = ref(0);
const page = ref(1);
const { settings } = useSettings();
/** 默认分页大小来自全局设置 */
const pageSize = ref(settings.value.page_size);
const errorMsg = ref("");
/** 导出进行中 */
const exporting = ref(false);
const exportOptions = [
  { label: "CSV", key: "csv" },
  { label: "Excel (.xlsx)", key: "xlsx" },
  { label: "Parquet", key: "parquet" },
];
/** 多条件筛选：每行一个字段+值，全部条件 AND 同时满足 */
interface FilterRow {
  field: number | null;
  value: string;
}
const filters = ref<FilterRow[]>([{ field: null, value: "" }]);

const hasAnyFilter = computed(() =>
  filters.value.some((f) => f.field !== null || f.value.trim())
);

/** 当前选中的 parquet key（由源文件 + sheet 共同决定） */
const currentKey = computed(() => {
  const g = groups.value.find((x) => x.source === selectedSource.value);
  if (!g) return "";
  const s = g.sheets.find((x) => x.sheet === selectedSheet.value);
  return s ? s.key : g.sheets[0]?.key ?? "";
});

const currentGroup = computed(
  () => groups.value.find((g) => g.source === selectedSource.value) ?? null
);

const fileOptions = computed(() =>
  groups.value.map((g) => {
    // 源文件名优先；无扩展名时退回 parquet 文件名，保证始终显示扩展名
    const label =
      g.source && g.source.includes(".")
        ? g.source
        : g.sheets[0]?.key || g.source || "未命名";
    return { label, value: g.source };
  })
);

const sheetOptions = computed(() =>
  currentGroup.value
    ? currentGroup.value.sheets.map((s) => ({
        label: s.sheet || "默认",
        value: s.sheet,
      }))
    : []
);

const tableColumns = computed(() =>
  columns.value.map((c, i) => ({
    title: c,
    key: `c${i}`,
    width: 140,
    ellipsis: { tooltip: true },
  }))
);

const tableData = computed(() =>
  rows.value.map((r, rowIdx) => ({
    __id: rowIdx,
    ...Object.fromEntries(r.map((v, i) => [`c${i}`, v])),
  }))
);

const rowKey = (row: { __id: number }) => row.__id;

const fieldOptions = computed(() =>
  columns.value.map((c, i) => ({ label: c, value: i }))
);

const scrollX = computed(() => columns.value.length * 140);

function setError(e: unknown) {
  errorMsg.value = e instanceof Error ? e.message : String(e);
}

/** 按当前源文件/sheet/分页/筛选条件向后端请求一页数据 */
async function loadPage() {
  const key = currentKey.value;
  if (!key) {
    rows.value = [];
    columns.value = [];
    total.value = 0;
    return;
  }
  loading.value = true;
  errorMsg.value = "";
  try {
    const reqFilters = filters.value
      .filter((f) => f.value.trim())
      .map((f) => ({ field: f.field, value: f.value.trim() }));
    const res = await invoke<ImportedFile>("read_parquet", {
      req: {
        key,
        offset: (page.value - 1) * pageSize.value,
        limit: pageSize.value,
        filters: reqFilters,
      },
    });
    columns.value = res.columns;
    rows.value = res.rows;
    total.value = res.row_count;
  } catch (e) {
    setError(e);
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

function onSelectFile(source: string) {
  selectedSource.value = source;
  const g = groups.value.find((x) => x.source === source);
  selectedSheet.value = g?.sheets[0]?.sheet ?? "";
  resetFilter();
  page.value = 1;
  loadPage();
}

function onSelectSheet(sheet: string) {
  selectedSheet.value = sheet;
  resetFilter();
  page.value = 1;
  loadPage();
}

function onPageChange(pageNum: number) {
  page.value = pageNum;
  loadPage();
}

function onUpdatePageSize(size: number) {
  pageSize.value = size;
  page.value = 1;
  loadPage();
}

/** 显式赋值某一行条件（避免 v-model 与 @update:value 叠加导致赋值被覆盖），不触发筛选 */
function onFieldInput(idx: number, v: number | null) {
  filters.value[idx].field = v;
}

function onValueInput(idx: number, v: string) {
  filters.value[idx].value = v;
}

function addFilter() {
  filters.value.push({ field: null, value: "" });
}

function removeFilter(idx: number) {
  if (filters.value.length > 1) {
    filters.value.splice(idx, 1);
  } else {
    filters.value[0] = { field: null, value: "" };
  }
}

/** 点击筛选按钮 / 回车：无任何条件时提示，否则立即筛选 */
function applyFilter() {
  if (!filters.value.some((f) => f.value.trim())) {
    message.info("请至少输入一个筛选条件");
    return;
  }
  page.value = 1;
  loadPage();
}

/** 仅重置筛选条件（不加载，供切换文件/sheet 时复用） */
function resetFilter() {
  filters.value = [{ field: null, value: "" }];
}

/** 清除筛选并重新加载全部数据 */
function clearFilter() {
  resetFilter();
  page.value = 1;
  loadPage();
}

onMounted(async () => {
  try {
    const metas = await invoke<
      { key: string; source: string; sheet: string }[]
    >("list_imported");
    const map = new Map<string, FileGroup>();
    for (const m of metas) {
      const src = m.source || m.key;
      if (!map.has(src)) map.set(src, { source: src, sheets: [] });
      map.get(src)!.sheets.push({ key: m.key, sheet: m.sheet });
    }
    groups.value = [...map.values()];
    if (groups.value.length) {
      selectedSource.value = groups.value[0].source;
      selectedSheet.value = groups.value[0].sheets[0].sheet;
      loadPage();
    }
  } catch (e) {
    setError(e);
    message.error(String(e));
  }
});

/** 导出当前预览的文件 */
async function onExport(format: string) {
  if (!currentKey.value) {
    message.info("请先选择文件");
    return;
  }
  const dir = await open({ directory: true, title: "选择导出目录" });
  if (!dir) return;
  exporting.value = true;
  try {
    await invoke("export_files", {
      req: {
        keys: [currentKey.value],
        format,
        output_dir: dir,
        merge: false,
        file_name: null,
      },
    });
    message.success(`已导出到 ${dir}`);
  } catch (e) {
    message.error(String(e));
  } finally {
    exporting.value = false;
  }
}

async function onUpload() {
  const selected = await open({
    multiple: true,
    filters: [{ name: "表格文件", extensions: ["xlsx", "xls", "csv"] }],
  });
  if (!selected) return;

  loading.value = true;
  errorMsg.value = "";
  try {
    for (const p of selected) {
      const results = await invoke<ImportedFile[]>("import_file", { path: p });
      for (const res of results) {
        let g = groups.value.find((x) => x.source === res.source);
        if (!g) {
          g = { source: res.source, sheets: [] };
          groups.value.push(g);
        }
        const idx = g.sheets.findIndex((s) => s.key === res.key);
        if (idx >= 0) g.sheets.splice(idx, 1);
        g.sheets.push({ key: res.key, sheet: res.sheet });
      }
      const last = results[results.length - 1];
      selectedSource.value = last.source;
      selectedSheet.value = last.sheet || "";
    }
    clearFilter();
    page.value = 1;
    await loadPage();
  } catch (e) {
    setError(e);
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>预览文件</h2>
    <p class="sub">上传 xlsx / xls / csv 文件，自动转换为 parquet 并预览数据。</p>

    <div class="preview-card">
      <div class="toolbar">
        <n-button type="primary" :loading="loading" @click="onUpload">
          上传文件
        </n-button>
        <n-select
          v-model:value="selectedSource"
          class="file-select"
          :options="fileOptions"
          :placeholder="fileOptions.length ? '选择文件' : '暂无文件，先上传'"
          :disabled="!fileOptions.length"
          @update:value="(s) => onSelectFile(s as string)"
        />
        <n-select
          v-model:value="selectedSheet"
          class="sheet-select"
          :options="sheetOptions"
          :disabled="sheetOptions.length <= 1"
          :placeholder="'选择工作表'"
          @update:value="(s) => onSelectSheet(s as string)"
        />
        <n-dropdown
          :options="exportOptions"
          @select="(k) => onExport(k as string)"
        >
          <n-button :disabled="!currentKey">导出</n-button>
        </n-dropdown>
      </div>

      <div v-if="currentKey" class="filter-bar">
        <div class="filter-head">
          <n-icon class="filter-icon"><FilterOutline /></n-icon>
          <span class="filter-label">筛选条件</span>
        </div>
        <div v-for="(f, idx) in filters" :key="idx" class="filter-row">
          <n-select
            :value="f.field"
            class="filter-field"
            :options="fieldOptions"
            placeholder="选择字段（可选）"
            clearable
            @update:value="(v) => onFieldInput(idx, v as number | null)"
          />
          <n-input
            :value="f.value"
            class="filter-value"
            placeholder="输入关键字"
            clearable
            @update:value="(v) => onValueInput(idx, v)"
            @keyup.enter="applyFilter"
          />
          <n-button
            v-if="filters.length > 1"
            text
            title="删除该条件"
            @click="removeFilter(idx)"
          >
            ✕
          </n-button>
        </div>
        <div class="filter-actions">
          <n-button text @click="addFilter">+ 添加条件</n-button>
          <n-button type="primary" size="small" @click="applyFilter">
            筛选
          </n-button>
          <n-button v-if="hasAnyFilter" text @click="clearFilter">
            清除
          </n-button>
          <span class="row-count">共 {{ total }} 行</span>
        </div>
      </div>

      <n-spin :show="loading">
        <div class="table-wrap">
          <n-data-table
            :key="currentKey"
            remote
            flex-height
            bordered
            :columns="tableColumns"
            :data="tableData"
            :row-key="rowKey"
            :pagination="{
              page,
              pageSize,
              itemCount: total,
              showSizePicker: true,
              pageSizes: [10, 20, 50, 100],
              onChange: onPageChange,
              onUpdatePageSize,
            }"
            :scroll-x="scrollX"
          />
          <p v-if="errorMsg" class="error-bar">加载失败：{{ errorMsg }}</p>
        </div>
      </n-spin>
    </div>

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

.preview-card {
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
  margin-bottom: 12px;
}

.file-select {
  width: 260px;
}

.sheet-select {
  width: 180px;
}

.filter-bar {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-bottom: 14px;
  padding: 12px;
  background: var(--brand-soft);
  border-radius: var(--radius-md);
}

.filter-head {
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.filter-icon {
  font-size: 16px;
  color: var(--brand);
  flex-shrink: 0;
}

.filter-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.filter-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.filter-field {
  width: 160px;
}

.filter-value {
  width: 220px;
}

.row-count {
  font-size: 12px;
  color: var(--text-muted);
  margin-left: auto;
}

/* 固定高度：表格区域自适应撑开，分页条恒定在右下角，数据不足时留白 */
.table-wrap {
  display: flex;
  flex-direction: column;
  height: 520px;
}

/* flex-height 生效前提：n-data-table 占满父容器高度 */
.table-wrap :deep(.n-data-table) {
  flex: 1;
  min-height: 0;
}

.table-wrap :deep(.n-data-table .n-data-table__pagination) {
  justify-content: flex-end;
  flex-shrink: 0;
}

.error-bar {
  padding: 8px 12px;
  margin-top: 8px;
  font-size: 12px;
  color: #ef4444;
  background: rgba(239, 68, 68, 0.08);
  border-radius: var(--radius-sm);
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
