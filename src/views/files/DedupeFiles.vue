<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useMessage } from "naive-ui";
import BackBar from "@components/common/BackBar.vue";
import { recordHistory } from "@/composables/useHistory";

interface FileGroup {
  source: string;
  sheets: { key: string; sheet: string }[];
}

interface ImportProgress {
  source: string;
  sheet: string;
  stage: "read" | "write";
  processed: number;
  total: number;
  sheet_index: number;
  sheet_count: number;
}

interface DedupeResult {
  key: string;
  total: number;
  kept: number;
}

const message = useMessage();
const router = useRouter();
const groups = ref<FileGroup[]>([]);

const source = ref("");
const sheet = ref("");
const key = ref("");
const cols = ref<string[]>([]);
const fields = ref<string[]>([]);
const outputName = ref("去重结果");

const running = ref(false);
const cancelling = ref(false);
const progress = ref<ImportProgress | null>(null);
const result = ref<DedupeResult | null>(null);
const showDone = ref(false);
const doneKey = ref("");
let unlistenProgress: UnlistenFn | undefined;

const fileOptions = computed(() =>
  groups.value.map((g) => ({ label: g.source, value: g.source }))
);

const progressPercent = computed(() =>
  progress.value?.total
    ? Math.min(
        100,
        Math.round((progress.value.processed / progress.value.total) * 100)
      )
    : 0
);

function sheetOptionsFor(src: string) {
  const g = groups.value.find((x) => x.source === src);
  return g
    ? g.sheets.map((s) => ({ label: s.sheet || "默认", value: s.sheet }))
    : [];
}

async function loadCols(k: string): Promise<string[]> {
  try {
    return await invoke<string[]>("get_columns", { key: k });
  } catch {
    return [];
  }
}

onMounted(async () => {
  unlistenProgress = await listen<ImportProgress>("import-progress", (e) => {
    progress.value = e.payload;
  });
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
  } catch (e) {
    message.error(String(e));
  }
});

onUnmounted(() => unlistenProgress?.());

function onSource(src: string) {
  const g = groups.value.find((x) => x.source === src);
  source.value = src;
  sheet.value = g?.sheets[0]?.sheet ?? "";
  key.value = g?.sheets[0]?.key ?? "";
  fields.value = [];
  loadCols(key.value).then((c) => (cols.value = c));
}

function onSheet(s: string) {
  const g = groups.value.find((x) => x.source === source.value);
  const sh = g?.sheets.find((x) => x.sheet === s);
  sheet.value = s;
  key.value = sh?.key ?? "";
  fields.value = [];
  loadCols(key.value).then((c) => (cols.value = c));
}

async function onCancel() {
  cancelling.value = true;
  try {
    await invoke("cancel_import");
  } catch {
    /* 取消命令失败可忽略 */
  }
}

function onGoPreview() {
  showDone.value = false;
  router.push({ path: "/files/preview", query: { key: doneKey.value } });
}

async function onRun() {
  if (!key.value) {
    message.info("请先选择文件");
    return;
  }
  running.value = true;
  cancelling.value = false;
  progress.value = null;
  result.value = null;
  try {
    const res = await invoke<DedupeResult>("dedupe_files", {
      req: {
        key: key.value,
        fields: fields.value,
        output_name: outputName.value,
      },
    });
    result.value = res;
    doneKey.value = res.key;
    showDone.value = true;
    recordHistory("dedupe", `去重 ${res.key}`, `/files/preview?key=${res.key}`);
  } catch (e) {
    if (cancelling.value) {
      message.info("去重已取消");
    } else {
      message.error(String(e));
    }
  } finally {
    running.value = false;
    cancelling.value = false;
    progress.value = null;
  }
}
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>数据去重</h2>
    <p class="sub">
      按所选字段组合判重，重复行保留首条；不选字段则按整行去重。结果生成新文件，原文件不变
    </p>

    <div class="card">
      <div class="section-title">目标文件</div>
      <div class="row">
        <n-select
          :value="source"
          class="file-select"
          :options="fileOptions"
          placeholder="选择文件"
          :disabled="!fileOptions.length"
          @update:value="(s) => onSource(s as string)"
        />
        <n-select
          :value="sheet"
          class="sheet-select"
          :options="sheetOptionsFor(source)"
          :disabled="sheetOptionsFor(source).length <= 1"
          placeholder="工作表"
          @update:value="(s) => onSheet(s as string)"
        />
        <n-select
          v-model:value="fields"
          multiple
          class="join-select"
          :options="cols.map((c) => ({ label: c, value: c }))"
          placeholder="去重依据字段（不选 = 整行）"
          :disabled="!cols.length"
          clearable
        />
      </div>
    </div>

    <div class="card">
      <div class="section-title">输出</div>
      <div class="row">
        <span class="label">输出文件名</span>
        <n-input
          v-model:value="outputName"
          class="name-input"
          placeholder="输出文件名（自动补 .parquet）"
        />
        <n-button type="primary" :disabled="!fileOptions.length" @click="onRun">
          执行去重
        </n-button>
      </div>
    </div>

    <div v-if="result" class="result-panel">
      <div class="result-head">
        <span class="result-title">去重完成</span>
      </div>
      <div class="result-meta">
        <span>总行数：{{ result.total.toLocaleString() }}</span>
        <span>保留：{{ result.kept.toLocaleString() }}</span>
        <span>删除：{{ (result.total - result.kept).toLocaleString() }}</span>
      </div>
      <div class="result-cols">新文件：{{ result.key }}</div>
    </div>

    <!-- 完成弹窗：确认跳转预览 -->
    <n-modal
      v-model:show="showDone"
      preset="dialog"
      title="去重完成"
      :positive-text="'去预览'"
      negative-text="留在本页"
      style="width: 400px"
      @positive-click="onGoPreview"
    >
      共 {{ result?.total.toLocaleString() }} 行，保留
      {{ result?.kept.toLocaleString() }} 行，是否前往预览页查看？
    </n-modal>

    <!-- 去重遮罩：全屏覆盖，期间无法操作其他页面 -->
    <div v-if="running" class="import-mask">
      <div class="mask-card">
        <n-spin size="large" />
        <p class="mask-title">正在去重…</p>
        <n-progress
          v-if="progress?.total"
          type="line"
          :percentage="progressPercent"
          :show-indicator="true"
          processing
          :height="8"
        />
        <p v-if="progress && !progress.total" class="mask-sub">
          已处理 {{ progress.processed.toLocaleString() }} 行…
        </p>
        <n-button secondary :loading="cancelling" @click="onCancel">
          {{ cancelling ? "正在取消…" : "取消去重" }}
        </n-button>
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
  margin-top: 16px;
  padding: 16px 20px;
  background: var(--surface);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  margin-bottom: 12px;
}

.row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.row:last-child {
  margin-bottom: 0;
}

.file-select {
  width: 220px;
}

.sheet-select {
  width: 140px;
}

.join-select {
  flex: 1;
  min-width: 160px;
}

.name-input {
  width: 300px;
}

.label {
  font-size: 13px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.result-panel {
  margin-top: 16px;
  padding: 16px;
  background: var(--brand-soft);
  border-radius: var(--radius-lg);
}

.result-head {
  margin-bottom: 10px;
}

.result-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
}

.result-meta {
  display: flex;
  gap: 20px;
  font-size: 13px;
  color: var(--text);
  margin-bottom: 10px;
}

.result-cols {
  font-size: 13px;
  color: var(--text-muted);
}

/* 去重遮罩：全屏覆盖，禁止操作其他页面 */
.import-mask {
  position: fixed;
  inset: 0;
  z-index: 2100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(15, 23, 42, 0.45);
}

.mask-card {
  min-width: 320px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  padding: 28px 32px;
  background: var(--surface);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
}

.mask-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
}

.mask-sub {
  font-size: 13px;
  color: var(--text-muted);
}
</style>
