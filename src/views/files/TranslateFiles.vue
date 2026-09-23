<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useMessage } from "naive-ui";
import BackBar from "@components/common/BackBar.vue";

interface FileGroup {
  source: string;
  sheets: { key: string; sheet: string }[];
}

/** 一个字典条目 */
interface DictEntry {
  source: string;
  sheet: string;
  key: string;
  cols: string[];
  dictFrom: string | null;
  dictTo: string | null;
}

/** 一个翻译字段：被翻译列 + 字典链（命中直接覆盖该列） */
interface FieldEntry {
  from: string | null;
  dicts: DictEntry[];
}

interface TranslateRound {
  dict: string;
  matched: number;
}

interface TranslateFieldResult {
  from: string;
  rounds: TranslateRound[];
}

interface TranslateResult {
  key: string;
  total: number;
  fields: TranslateFieldResult[];
}

/** 后端 import-progress 事件（翻译任务复用同一事件） */
interface ImportProgress {
  source: string;
  sheet: string;
  stage: "read" | "write";
  processed: number;
  total: number;
  sheet_index: number;
  sheet_count: number;
}

function emptyDict(): DictEntry {
  return {
    source: "",
    sheet: "",
    key: "",
    cols: [],
    dictFrom: null,
    dictTo: null,
  };
}

function emptyField(): FieldEntry {
  return { from: null, dicts: [emptyDict()] };
}

const message = useMessage();
const router = useRouter();
const groups = ref<FileGroup[]>([]);

// 翻译字段列表（每字段带自己的字典链）
const fields = ref<FieldEntry[]>([emptyField()]);

// 目标文件
const targetSource = ref("");
const targetSheet = ref("");
const targetKey = ref("");
const targetCols = ref<string[]>([]);

const outputName = ref("翻译结果.parquet");

const translating = ref(false);
const cancelling = ref(false);
const progress = ref<ImportProgress | null>(null);
const result = ref<TranslateResult | null>(null);
/** 翻译完成弹窗：询问是否跳转预览 */
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

async function loadCols(key: string): Promise<string[]> {
  try {
    return await invoke<string[]>("get_columns", { key });
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

// 字段增删
function addField() {
  fields.value.push(emptyField());
}

function removeField(idx: number) {
  if (fields.value.length > 1) {
    fields.value.splice(idx, 1);
  } else {
    fields.value[0] = emptyField();
  }
}

function addDict(fi: number) {
  fields.value[fi].dicts.push(emptyDict());
}

function removeDict(fi: number, di: number) {
  const dicts = fields.value[fi].dicts;
  if (dicts.length > 1) {
    dicts.splice(di, 1);
  } else {
    dicts[0] = emptyDict();
  }
}

// 字典文件选择
function onDictSource(fi: number, di: number, src: string) {
  const g = groups.value.find((x) => x.source === src);
  const d = fields.value[fi].dicts[di];
  d.source = src;
  d.sheet = g?.sheets[0]?.sheet ?? "";
  d.key = g?.sheets[0]?.key ?? "";
  d.dictFrom = null;
  d.dictTo = null;
  loadCols(d.key).then((c) => (d.cols = c));
}

function onDictSheet(fi: number, di: number, sheet: string) {
  const d = fields.value[fi].dicts[di];
  const g = groups.value.find((x) => x.source === d.source);
  const s = g?.sheets.find((x) => x.sheet === sheet);
  d.sheet = sheet;
  d.key = s?.key ?? "";
  d.dictFrom = null;
  d.dictTo = null;
  loadCols(d.key).then((c) => (d.cols = c));
}

// 目标文件选择
function onTargetSource(src: string) {
  const g = groups.value.find((x) => x.source === src);
  targetSource.value = src;
  targetSheet.value = g?.sheets[0]?.sheet ?? "";
  targetKey.value = g?.sheets[0]?.key ?? "";
  loadCols(targetKey.value).then((c) => (targetCols.value = c));
}

function onTargetSheet(sheet: string) {
  const g = groups.value.find((x) => x.source === targetSource.value);
  const s = g?.sheets.find((x) => x.sheet === sheet);
  targetSheet.value = sheet;
  targetKey.value = s?.key ?? "";
  loadCols(targetKey.value).then((c) => (targetCols.value = c));
}

async function onCancel() {
  cancelling.value = true;
  try {
    await invoke("cancel_import");
  } catch {
    /* 取消命令失败可忽略，任务循环仍会收到标志 */
  }
}

/** 翻译完成弹窗：确认后跳转预览并自动选中新文件 */
function onGoPreview() {
  showDone.value = false;
  router.push({ path: "/files/preview", query: { key: doneKey.value } });
}

async function onStart() {
  if (!targetKey.value) {
    message.info("请先选择目标文件");
    return;
  }
  for (let i = 0; i < fields.value.length; i++) {
    const f = fields.value[i];
    if (!f.from) {
      message.info(`请为第 ${i + 1} 个字段选择被翻译列`);
      return;
    }
    for (let j = 0; j < f.dicts.length; j++) {
      const d = f.dicts[j];
      if (!d.key || !d.dictFrom || !d.dictTo) {
        message.info(
          `请完整选择第 ${i + 1} 个字段的第 ${j + 1} 个字典的匹配列与译文列`
        );
        return;
      }
    }
  }
  translating.value = true;
  cancelling.value = false;
  progress.value = null;
  result.value = null;
  try {
    const res = await invoke<TranslateResult>("translate_file", {
      req: {
        target_key: targetKey.value,
        fields: fields.value.map((f) => ({
          from: f.from,
          steps: f.dicts.map((d) => ({
            dict_key: d.key,
            dict_from: d.dictFrom,
            dict_to: d.dictTo,
          })),
        })),
        output_name: outputName.value,
      },
    });
    result.value = res;
    doneKey.value = res.key;
    showDone.value = true;
  } catch (e) {
    if (cancelling.value) {
      message.info("翻译已取消");
    } else {
      message.error(String(e));
    }
  } finally {
    translating.value = false;
    cancelling.value = false;
    progress.value = null;
  }
}
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>文件翻译</h2>
    <p class="sub">
      目标文件可选多个翻译字段，每个字段可配多个字典并按顺序链式执行（每轮读该列当前值继续查）；译文直接覆盖被翻译列，未匹配的行保留原值，结果生成新文件（原文件不变）
    </p>

    <div class="card">
      <div class="section-title">目标文件</div>
      <div class="row">
        <n-select
          :value="targetSource"
          class="file-select"
          :options="fileOptions"
          placeholder="选择目标文件"
          :disabled="!fileOptions.length"
          @update:value="(s) => onTargetSource(s as string)"
        />
        <n-select
          :value="targetSheet"
          class="sheet-select"
          :options="sheetOptionsFor(targetSource)"
          :disabled="sheetOptionsFor(targetSource).length <= 1"
          placeholder="工作表"
          @update:value="(s) => onTargetSheet(s as string)"
        />
        <span class="label">选择后，其列可作为下方翻译字段的选项</span>
      </div>
    </div>

    <div class="card">
      <div class="section-title">
        翻译字段
        <n-button text type="primary" @click="addField">+ 添加翻译字段</n-button>
      </div>
      <div v-for="(f, fi) in fields" :key="fi" class="field-block">
        <div class="field-head">
          <span class="field-index">字段 {{ fi + 1 }}</span>
          <n-button
            v-if="fields.length > 1"
            text
            type="error"
            size="small"
            @click="removeField(fi)"
          >
            删除
          </n-button>
        </div>
        <div class="row">
          <span class="label">被翻译列</span>
          <n-select
            v-model:value="f.from"
            class="join-select"
            :options="targetCols.map((c) => ({ label: c, value: c }))"
            placeholder="选择要翻译的列（译文直接覆盖该列）"
            :disabled="!targetCols.length"
          />
        </div>

        <div class="dict-list">
          <div class="dict-title-row">
            <span class="dict-title">字典链（按顺序执行）</span>
            <n-button text type="primary" size="small" @click="addDict(fi)">
              + 添加字典
            </n-button>
          </div>
          <div v-for="(d, di) in f.dicts" :key="di" class="dict-row">
            <span class="dict-index">字典 {{ di + 1 }}</span>
            <n-select
              :value="d.source"
              class="file-select"
              :options="fileOptions"
              placeholder="选择翻译文件"
              :disabled="!fileOptions.length"
              @update:value="(s) => onDictSource(fi, di, s as string)"
            />
            <n-select
              :value="d.sheet"
              class="sheet-select"
              :options="sheetOptionsFor(d.source)"
              :disabled="sheetOptionsFor(d.source).length <= 1"
              placeholder="工作表"
              @update:value="(s) => onDictSheet(fi, di, s as string)"
            />
            <n-select
              v-model:value="d.dictFrom"
              class="dict-col"
              :options="d.cols.map((c) => ({ label: c, value: c }))"
              placeholder="匹配列（原文）"
              :disabled="!d.cols.length"
            />
            <span class="arrow">→</span>
            <n-select
              v-model:value="d.dictTo"
              class="dict-col"
              :options="d.cols.map((c) => ({ label: c, value: c }))"
              placeholder="译文列"
              :disabled="!d.cols.length"
            />
            <n-button
              v-if="f.dicts.length > 1"
              text
              size="small"
              title="删除该字典"
              @click="removeDict(fi, di)"
            >
              ✕
            </n-button>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="section-title">输出</div>
      <div class="row">
        <span class="label">输出文件名</span>
        <n-input
          v-model:value="outputName"
          class="name-input"
          placeholder="翻译结果.parquet"
        />
        <n-button
          type="primary"
          :disabled="!fileOptions.length"
          @click="onStart"
        >
          开始翻译
        </n-button>
      </div>
    </div>

    <div v-if="result" class="result-panel">
      <div class="result-head">
        <span class="result-title">翻译完成</span>
        <span class="result-total">总行数：{{ result.total.toLocaleString() }}</span>
      </div>
      <div v-for="(fr, i) in result.fields" :key="i" class="field-result">
        <span class="field-result-name">{{ fr.from }}：</span>
        <span v-for="(r, j) in fr.rounds" :key="j" class="round-item">
          {{ r.dict }} 匹配 {{ r.matched.toLocaleString() }}
          <template v-if="j < fr.rounds.length - 1"> · </template>
        </span>
      </div>
      <div class="result-cols">新文件：{{ result.key }}</div>
    </div>

    <!-- 翻译完成弹窗：确认跳转预览 -->
    <n-modal
      v-model:show="showDone"
      preset="dialog"
      title="翻译完成"
      :positive-text="'去预览'"
      negative-text="留在本页"
      style="width: 400px"
      @positive-click="onGoPreview"
    >
      新文件「{{ doneKey }}」已生成，是否前往预览页查看？
    </n-modal>

    <!-- 翻译遮罩：全屏覆盖，期间无法操作其他页面 -->
    <div v-if="translating" class="import-mask">
      <div class="mask-card">
        <n-spin size="large" />
        <p class="mask-title">
          {{ progress?.stage === "write" ? "正在翻译" : "正在读取字典" }}…
        </p>
        <template v-if="progress && progress.stage === 'write'">
          <n-progress
            v-if="progress.total"
            type="line"
            :percentage="progressPercent"
            :show-indicator="true"
            processing
            :height="8"
          />
          <p v-else class="mask-sub">
            已处理 {{ progress.processed.toLocaleString() }} 行…
          </p>
        </template>
        <p v-if="progress && progress.stage === 'read'" class="mask-sub">
          {{ progress.sheet }}（{{ progress.sheet_index }} /
          {{ progress.sheet_count }}）
        </p>
        <n-button secondary :loading="cancelling" @click="onCancel">
          {{ cancelling ? "正在取消…" : "取消翻译" }}
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

.field-block {
  padding: 12px;
  margin-bottom: 12px;
  background: var(--bg);
  border-radius: var(--radius-md);
}

.field-block:last-child {
  margin-bottom: 0;
}

.field-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.field-index {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.dict-list {
  margin-top: 8px;
  padding: 10px 12px;
  background: var(--surface);
  border-radius: var(--radius-sm);
}

.dict-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.dict-title {
  font-size: 12px;
  color: var(--text-muted);
}

.dict-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.dict-row:last-child {
  margin-bottom: 0;
}

.dict-index {
  font-size: 12px;
  color: var(--text-muted);
  flex-shrink: 0;
  width: 48px;
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
  width: 180px;
}

.sheet-select {
  width: 130px;
}

.join-select {
  flex: 1;
  min-width: 160px;
}

.dict-col {
  flex: 1;
  min-width: 130px;
}

.name-input {
  width: 300px;
}

.arrow {
  color: var(--text-muted);
  flex-shrink: 0;
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
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 10px;
}

.result-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
}

.result-total {
  font-size: 13px;
  color: var(--text-muted);
}

.field-result {
  font-size: 13px;
  color: var(--text);
  margin-bottom: 6px;
}

.field-result-name {
  font-weight: 600;
}

.round-item {
  color: var(--text-muted);
}

.result-cols {
  font-size: 13px;
  color: var(--text-muted);
  margin-top: 8px;
}

/* 翻译遮罩：全屏覆盖，禁止操作其他页面 */
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
