<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { useMessage } from "naive-ui";
import BackBar from "@components/common/BackBar.vue";

interface FileGroup {
  source: string;
  sheets: { key: string; sheet: string }[];
}

interface SecondaryFile {
  source: string;
  sheet: string;
  key: string;
  cols: string[];
  joins: { sec_field: string; main_field: string }[];
}

const message = useMessage();
const router = useRouter();
const groups = ref<FileGroup[]>([]);
const merging = ref(false);
/** 融合结果反馈 */
const mergedResult = ref<{ key: string; row_count: number; columns: string[] } | null>(null);

// 主文件
const mainSource = ref("");
const mainSheet = ref("");
const mainKey = ref("");
const mainCols = ref<string[]>([]);
const mainJoinFields = ref<string[]>([]);

const secondaries = ref<SecondaryFile[]>([]);
const outputName = ref("融合结果.parquet");
const joinType = ref("inner");

const fileOptions = computed(() =>
  groups.value.map((g) => ({ label: g.source, value: g.source }))
);

function sheetOptionsFor(src: string) {
  const g = groups.value.find((x) => x.source === src);
  return g
    ? g.sheets.map((s) => ({ label: s.sheet || "默认", value: s.sheet }))
    : [];
}

/** 主文件连接字段变化时，清理副文件映射中已失效的主字段 */
function onMainJoinFields(v: string[]) {
  mainJoinFields.value = v;
  for (const sec of secondaries.value) {
    sec.joins = sec.joins.map((j) =>
      v.includes(j.main_field) ? j : { sec_field: j.sec_field, main_field: "" }
    );
  }
}

async function loadCols(key: string): Promise<string[]> {
  try {
    return await invoke<string[]>("get_columns", { key });
  } catch {
    return [];
  }
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
  } catch (e) {
    message.error(String(e));
  }
});

// 主文件选择
function onMainSource(src: string) {
  const g = groups.value.find((x) => x.source === src);
  mainSource.value = src;
  mainSheet.value = g?.sheets[0]?.sheet ?? "";
  mainKey.value = g?.sheets[0]?.key ?? "";
  mainJoinFields.value = [];
  loadCols(mainKey.value).then((c) => (mainCols.value = c));
}

function onMainSheet(sheet: string) {
  const g = groups.value.find((x) => x.source === mainSource.value);
  const s = g?.sheets.find((x) => x.sheet === sheet);
  mainSheet.value = sheet;
  mainKey.value = s?.key ?? "";
  mainJoinFields.value = [];
  loadCols(mainKey.value).then((c) => (mainCols.value = c));
}

// 副文件
function addSecondary() {
  secondaries.value.push({
    source: "",
    sheet: "",
    key: "",
    cols: [],
    joins: [],
  });
}

function removeSecondary(idx: number) {
  secondaries.value.splice(idx, 1);
}

function onSecSource(idx: number, src: string) {
  const g = groups.value.find((x) => x.source === src);
  const sec = secondaries.value[idx];
  sec.source = src;
  sec.sheet = g?.sheets[0]?.sheet ?? "";
  sec.key = g?.sheets[0]?.key ?? "";
  sec.joins = [];
  loadCols(sec.key).then((c) => (sec.cols = c));
}

function onSecSheet(idx: number, sheet: string) {
  const g = groups.value.find((x) => x.source === secondaries.value[idx].source);
  const s = g?.sheets.find((x) => x.sheet === sheet);
  const sec = secondaries.value[idx];
  sec.sheet = sheet;
  sec.key = s?.key ?? "";
  sec.joins = [];
  loadCols(sec.key).then((c) => (sec.cols = c));
}

async function onMerge() {
  if (!mainKey.value) {
    message.info("请选择主文件");
    return;
  }
  if (!mainJoinFields.value.length) {
    message.info("请选择主文件的连接字段");
    return;
  }
  const validSec = secondaries.value.filter((s) => s.key);
  if (!validSec.length) {
    message.info("请至少添加一个副文件并选择文件");
    return;
  }
  for (const s of validSec) {
    const validJoins = s.joins.filter((j) => j.sec_field && j.main_field);
    if (!validJoins.length) {
      message.info("请为每个副文件配置连接字段映射");
      return;
    }
    s.joins = validJoins;
  }
  merging.value = true;
  try {
    const res = await invoke<{ key: string; row_count: number; columns: string[] }>(
      "merge_files",
      {
        req: {
          main_key: mainKey.value,
          main_join_fields: mainJoinFields.value,
          secondaries: validSec.map((s) => ({
            key: s.key,
            joins: s.joins.map((j) => ({
              sec_field: j.sec_field,
              main_field: j.main_field,
            })),
          })),
          output_name: outputName.value,
          join_type: joinType.value,
        },
      }
    );
    message.success(`融合完成，共 ${res.row_count} 行`);
    mergedResult.value = res;
  } catch (e) {
    message.error(String(e));
  } finally {
    merging.value = false;
  }
}
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>文件融合</h2>
    <p class="sub">以主文件为基准，与多个副文件按字段一对一合并，同名字段自动加前缀区分（A_, B_ 等）</p>

    <div class="card">
      <div class="section-title">主文件</div>
      <div class="row">
        <n-select
          :value="mainSource"
          class="file-select"
          :options="fileOptions"
          placeholder="选择主文件"
          :disabled="!fileOptions.length"
          @update:value="(s) => onMainSource(s as string)"
        />
        <n-select
          :value="mainSheet"
          class="sheet-select"
          :options="sheetOptionsFor(mainSource)"
          :disabled="sheetOptionsFor(mainSource).length <= 1"
          placeholder="工作表"
          @update:value="(s) => onMainSheet(s as string)"
        />
        <n-select
          :value="mainJoinFields"
          class="join-select main-join"
          multiple
          :bordered="true"
          :options="mainCols.map((c) => ({ label: c, value: c }))"
          placeholder="选择连接字段"
          @update:value="(v) => onMainJoinFields(v as string[])"
        />
      </div>
      <div class="row">
        <span class="label">连接方式</span>
        <n-radio-group v-model:value="joinType" size="small">
          <n-radio-button value="inner">内连接</n-radio-button>
          <n-radio-button value="left">左连接</n-radio-button>
          <n-radio-button value="right">右连接</n-radio-button>
        </n-radio-group>
      </div>
    </div>

    <div class="card">
      <div class="section-title">
        副文件
        <n-button text type="primary" @click="addSecondary">+ 添加副文件</n-button>
      </div>
      <div v-for="(sec, idx) in secondaries" :key="idx" class="sec-block">
        <div class="row">
          <n-select
            :value="sec.source"
            class="file-select"
            :options="fileOptions"
            placeholder="选择副文件"
            @update:value="(s) => onSecSource(idx, s as string)"
          />
          <n-select
            :value="sec.sheet"
            class="sheet-select"
            :options="sheetOptionsFor(sec.source)"
            :disabled="sheetOptionsFor(sec.source).length <= 1"
            placeholder="工作表"
            @update:value="(s) => onSecSheet(idx, s as string)"
          />
          <n-button text type="error" @click="removeSecondary(idx)">
            删除副文件
          </n-button>
        </div>
        <div
          v-for="(j, jidx) in sec.joins"
          :key="jidx"
          class="row join-row"
        >
          <n-select
            :value="j.sec_field"
            class="join-select"
            :options="sec.cols.map((c) => ({ label: c, value: c }))"
            placeholder="副文件字段"
            @update:value="(v) => (j.sec_field = v as string)"
          />
          <span class="arrow">→</span>
          <n-select
            :value="j.main_field"
            class="join-select"
            :options="mainJoinFields.map((f) => ({ label: f, value: f }))"
            placeholder="主文件字段"
            @update:value="(v) => (j.main_field = v as string)"
          />
          <n-button text @click="sec.joins.splice(jidx, 1)">✕</n-button>
        </div>
        <n-button
          text
          type="primary"
          @click="sec.joins.push({ sec_field: '', main_field: '' })"
        >
          + 连接字段
        </n-button>
      </div>
      <n-empty v-if="!secondaries.length" description="尚未添加副文件" size="small" />
    </div>

    <div class="card">
      <div class="row">
        <n-input
          v-model:value="outputName"
          class="name-input"
          placeholder="输出文件名（自动补 .parquet）"
        />
        <n-button type="primary" :loading="merging" @click="onMerge">
          执行融合
        </n-button>
      </div>
    </div>

    <div v-if="mergedResult" class="result-panel">
      <div class="result-head">
        <span class="result-title">融合结果</span>
        <n-button
          type="primary"
          size="small"
          @click="router.push('/files/preview')"
        >
          跳转预览
        </n-button>
      </div>
      <div class="result-meta">
        <span>文件名：{{ mergedResult.key }}</span>
        <span>行数：{{ mergedResult.row_count }}</span>
        <span>字段数：{{ mergedResult.columns.length }}</span>
      </div>
      <div class="result-cols">
        <n-tag v-for="c in mergedResult.columns" :key="c" size="small">
          {{ c }}
        </n-tag>
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

.main-join :deep(.n-base-selection) {
  background: transparent;
  border-radius: var(--radius-md);
}

.sec-block {
  padding: 12px;
  margin-bottom: 12px;
  background: var(--bg);
  border-radius: var(--radius-md);
}

.join-row {
  padding-left: 12px;
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
  justify-content: space-between;
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
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  max-height: 120px;
  overflow-y: auto;
}
</style>
