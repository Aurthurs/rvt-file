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

/** 一条变换规则 */
interface RuleEntry {
  field: string | null;
  op: string;
  param: string;
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

interface TransformResult {
  key: string;
  rows: number;
}

function emptyRule(): RuleEntry {
  return { field: null, op: "trim", param: "" };
}

const message = useMessage();
const router = useRouter();
const groups = ref<FileGroup[]>([]);

const source = ref("");
const sheet = ref("");
const key = ref("");
const cols = ref<string[]>([]);
const rules = ref<RuleEntry[]>([emptyRule()]);
const outputName = ref("变换结果");

const running = ref(false);
const cancelling = ref(false);
const progress = ref<ImportProgress | null>(null);
const result = ref<TransformResult | null>(null);
const showDone = ref(false);
const doneKey = ref("");
let unlistenProgress: UnlistenFn | undefined;

const opOptions = [
  { label: "去首尾空格", value: "trim" },
  { label: "转大写", value: "upper" },
  { label: "转小写", value: "lower" },
  { label: "替换", value: "replace" },
  { label: "拼接", value: "concat" },
  { label: "拆分", value: "split" },
];

/** 各操作的参数提示（无参数的操作显示占位说明） */
const opHints: Record<string, string> = {
  trim: "无参数",
  upper: "无参数",
  lower: "无参数",
  replace: "格式：旧值,新值",
  concat: "格式：拼接列,分隔符",
  split: "格式：分隔符,新列名",
};

function needsParam(op: string) {
  return op === "replace" || op === "concat" || op === "split";
}

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
  rules.value = [emptyRule()];
  loadCols(key.value).then((c) => (cols.value = c));
}

function onSheet(s: string) {
  const g = groups.value.find((x) => x.source === source.value);
  const sh = g?.sheets.find((x) => x.sheet === s);
  sheet.value = s;
  key.value = sh?.key ?? "";
  rules.value = [emptyRule()];
  loadCols(key.value).then((c) => (cols.value = c));
}

function addRule() {
  rules.value.push(emptyRule());
}

function removeRule(idx: number) {
  if (rules.value.length > 1) {
    rules.value.splice(idx, 1);
  } else {
    rules.value[0] = emptyRule();
  }
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
  for (let i = 0; i < rules.value.length; i++) {
    const r = rules.value[i];
    if (!r.field) {
      message.info(`请为第 ${i + 1} 条规则选择目标字段`);
      return;
    }
    if (needsParam(r.op) && !r.param.includes(",")) {
      message.info(`第 ${i + 1} 条规则：${opHints[r.op]}`);
      return;
    }
  }
  running.value = true;
  cancelling.value = false;
  progress.value = null;
  result.value = null;
  try {
    const res = await invoke<TransformResult>("transform_files", {
      req: {
        key: key.value,
        ops: rules.value.map((r) => ({
          field: r.field,
          op: r.op,
          param: r.param,
        })),
        output_name: outputName.value,
      },
    });
    result.value = res;
    doneKey.value = res.key;
    showDone.value = true;
    recordHistory("transform", `变换 ${res.key}`, `/files/preview?key=${res.key}`);
  } catch (e) {
    if (cancelling.value) {
      message.info("变换已取消");
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
    <h2>列规则变换</h2>
    <p class="sub">
      对目标文件的列按规则批量改写：去空格、大小写、替换、拼接、拆分；规则按顺序逐行执行，结果生成新文件
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
      </div>
    </div>

    <div class="card">
      <div class="section-title">
        变换规则（按顺序执行）
        <n-button text type="primary" @click="addRule">+ 添加规则</n-button>
      </div>
      <div v-for="(r, idx) in rules" :key="idx" class="rule-row">
        <span class="rule-index">规则 {{ idx + 1 }}</span>
        <n-select
          v-model:value="r.field"
          class="field-select"
          :options="cols.map((c) => ({ label: c, value: c }))"
          placeholder="目标字段"
          :disabled="!cols.length"
        />
        <n-select
          v-model:value="r.op"
          class="op-select"
          :options="opOptions"
        />
        <n-input
          v-model:value="r.param"
          class="param-input"
          :placeholder="opHints[r.op]"
          :disabled="!needsParam(r.op)"
        />
        <n-button
          v-if="rules.length > 1"
          text
          size="small"
          title="删除该规则"
          @click="removeRule(idx)"
        >
          ✕
        </n-button>
      </div>
      <p class="tip">
        示例：替换 <code>旧值,新值</code>；拼接 <code>列B,分隔符</code>
        （写回目标列）；拆分 <code>分隔符,新列名</code>（第一段留原列，第二段进新列）
      </p>
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
          执行变换
        </n-button>
      </div>
    </div>

    <div v-if="result" class="result-panel">
      <div class="result-head">
        <span class="result-title">变换完成</span>
      </div>
      <div class="result-meta">
        <span>处理 {{ result.rows.toLocaleString() }} 行</span>
      </div>
      <div class="result-cols">新文件：{{ result.key }}</div>
    </div>

    <!-- 完成弹窗：确认跳转预览 -->
    <n-modal
      v-model:show="showDone"
      preset="dialog"
      title="变换完成"
      :positive-text="'去预览'"
      negative-text="留在本页"
      style="width: 400px"
      @positive-click="onGoPreview"
    >
      已处理 {{ result?.rows.toLocaleString() }} 行，是否前往预览页查看？
    </n-modal>

    <!-- 变换遮罩：全屏覆盖，期间无法操作其他页面 -->
    <div v-if="running" class="import-mask">
      <div class="mask-card">
        <n-spin size="large" />
        <p class="mask-title">正在变换…</p>
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
          {{ cancelling ? "正在取消…" : "取消变换" }}
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
  display: flex;
  align-items: center;
  justify-content: space-between;
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

.rule-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.rule-row:last-of-type {
  margin-bottom: 12px;
}

.rule-index {
  font-size: 12px;
  color: var(--text-muted);
  flex-shrink: 0;
  width: 52px;
}

.field-select {
  width: 180px;
}

.op-select {
  width: 130px;
}

.param-input {
  flex: 1;
  min-width: 180px;
}

.file-select {
  width: 220px;
}

.sheet-select {
  width: 140px;
}

.name-input {
  width: 300px;
}

.label {
  font-size: 13px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.tip {
  font-size: 12px;
  color: var(--text-muted);
}

.tip code {
  padding: 1px 4px;
  background: var(--brand-soft);
  border-radius: var(--radius-sm);
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

/* 变换遮罩：全屏覆盖，禁止操作其他页面 */
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
