<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useMessage } from "naive-ui";
import BackBar from "@components/common/BackBar.vue";

interface FieldQuality {
  name: string;
  total: number;
  non_null: number;
  null_rate: number;
  unique: number;
  duplicates: number;
  min: string | null;
  max: string | null;
  avg_len: number;
  max_len: number;
}

interface FileGroup {
  source: string;
  sheets: { key: string; sheet: string }[];
}

const message = useMessage();
const loading = ref(false);
const groups = ref<FileGroup[]>([]);
const selectedSource = ref("");
const selectedSheet = ref("");
const fields = ref<FieldQuality[]>([]);

const currentKey = computed(() => {
  const g = groups.value.find((x) => x.source === selectedSource.value);
  if (!g) return "";
  const s = g.sheets.find((x) => x.sheet === selectedSheet.value);
  return s ? s.key : g.sheets[0]?.key ?? "";
});

const fileOptions = computed(() =>
  groups.value.map((g) => ({ label: g.source, value: g.source }))
);

const currentGroup = computed(
  () => groups.value.find((g) => g.source === selectedSource.value) ?? null
);

const sheetOptions = computed(() =>
  currentGroup.value
    ? currentGroup.value.sheets.map((s) => ({
        label: s.sheet || "默认",
        value: s.sheet,
      }))
    : []
);

// 所有列仅设 minWidth，不设固定宽，表格弹性均分列宽；超出列宽省略号 + tooltip
const ellipsis = { tooltip: true } as const;
const columns = [
  { title: "字段名", key: "name", minWidth: 100, ellipsis },
  {
    title: "非空/总数",
    key: "non_null",
    minWidth: 90,
    ellipsis,
    render: (r: FieldQuality) => `${r.non_null} / ${r.total}`,
  },
  {
    title: "空值率",
    key: "null_rate",
    minWidth: 80,
    ellipsis,
    render: (r: FieldQuality) => `${r.null_rate.toFixed(1)}%`,
  },
  { title: "唯一值", key: "unique", minWidth: 70, ellipsis },
  { title: "重复值", key: "duplicates", minWidth: 70, ellipsis },
  {
    title: "最小",
    key: "min",
    minWidth: 90,
    ellipsis,
    render: (r: FieldQuality) => r.min ?? "-",
  },
  {
    title: "最大",
    key: "max",
    minWidth: 90,
    ellipsis,
    render: (r: FieldQuality) => r.max ?? "-",
  },
  {
    title: "平均长度",
    key: "avg_len",
    minWidth: 80,
    ellipsis,
    render: (r: FieldQuality) => r.avg_len.toFixed(1),
  },
  { title: "最大长度", key: "max_len", minWidth: 80, ellipsis },
];

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
    }
  } catch (e) {
    message.error(String(e));
  }
});

async function onScan() {
  if (!currentKey.value) {
    message.info("请先选择文件");
    return;
  }
  loading.value = true;
  try {
    fields.value = await invoke<FieldQuality[]>("scan_quality", {
      req: { key: currentKey.value },
    });
    if (!fields.value.length) message.info("未扫描到字段");
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>质量检测</h2>
    <p class="sub">扫描数据，检查每个字段的质量与统计情况。</p>

    <div class="card">
      <div class="toolbar">
        <n-select
          v-model:value="selectedSource"
          class="file-select"
          :options="fileOptions"
          placeholder="选择文件"
          :disabled="!fileOptions.length"
          @update:value="(s) => (selectedSource = s as string)"
        />
        <n-select
          v-model:value="selectedSheet"
          class="sheet-select"
          :options="sheetOptions"
          :disabled="sheetOptions.length <= 1"
          placeholder="选择工作表"
          @update:value="(s) => (selectedSheet = s as string)"
        />
        <n-button type="primary" :loading="loading" @click="onScan">
          开始扫描
        </n-button>
      </div>

      <div class="table-wrap">
        <n-data-table
          flex-height
          bordered
          :columns="columns"
          :data="fields"
          :row-key="(row: FieldQuality) => row.name"
          :scroll-x="columns.length * 100"
          :pagination="{ pageSize: 10 }"
        />
      </div>
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

.file-select {
  width: 260px;
}

.sheet-select {
  width: 180px;
}

/* 与预览页一致的固定高度：数据多时表格内部滚动，分页条固定底部 */
.table-wrap {
  display: flex;
  flex-direction: column;
  height: 520px;
}

.table-wrap :deep(.n-data-table) {
  flex: 1;
  min-height: 0;
}

.table-wrap :deep(.n-data-table .n-data-table__pagination) {
  justify-content: flex-end;
  flex-shrink: 0;
}
</style>
