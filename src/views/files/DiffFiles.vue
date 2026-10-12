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

interface DiffResult {
  key: string;
  only_left: number;
  only_right: number;
  changed: number;
  same: number;
}

const message = useMessage();
const router = useRouter();
const groups = ref<FileGroup[]>([]);

// 左文件
const leftSource = ref("");
const leftSheet = ref("");
const leftKey = ref("");
const leftCols = ref<string[]>([]);
const leftFields = ref<string[]>([]);

// 右文件
const rightSource = ref("");
const rightSheet = ref("");
const rightKey = ref("");
const rightCols = ref<string[]>([]);
const rightFields = ref<string[]>([]);

const outputName = ref("对账结果");

const running = ref(false);
const cancelling = ref(false);
const progress = ref<ImportProgress | null>(null);
const result = ref<DiffResult | null>(null);
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

// 左文件选择
function onLeftSource(src: string) {
  const g = groups.value.find((x) => x.source === src);
  leftSource.value = src;
  leftSheet.value = g?.sheets[0]?.sheet ?? "";
  leftKey.value = g?.sheets[0]?.key ?? "";
  leftFields.value = [];
  loadCols(leftKey.value).then((c) => (leftCols.value = c));
}

function onLeftSheet(sheet: string) {
  const g = groups.value.find((x) => x.source === leftSource.value);
  const s = g?.sheets.find((x) => x.sheet === sheet);
  leftSheet.value = sheet;
  leftKey.value = s?.key ?? "";
  leftFields.value = [];
  loadCols(leftKey.value).then((c) => (leftCols.value = c));
}

// 左主键变化时，右主键按同名字段自动预填
function onLeftFields(v: Array<string | number>) {
  leftFields.value = v.map(String);
  rightFields.value = leftFields.value.filter((f) =>
    rightCols.value.includes(f)
  );
}

// 右文件选择
function onRightSource(src: string) {
  const g = groups.value.find((x) => x.source === src);
  rightSource.value = src;
  rightSheet.value = g?.sheets[0]?.sheet ?? "";
  rightKey.value = g?.sheets[0]?.key ?? "";
  rightFields.value = leftFields.value.filter((f) =>
    rightCols.value.includes(f)
  );
  loadCols(rightKey.value).then((c) => {
    rightCols.value = c;
    rightFields.value = leftFields.value.filter((f) => c.includes(f));
  });
}

function onRightSheet(sheet: string) {
  const g = groups.value.find((x) => x.source === rightSource.value);
  const s = g?.sheets.find((x) => x.sheet === sheet);
  rightSheet.value = sheet;
  rightKey.value = s?.key ?? "";
  loadCols(rightKey.value).then((c) => {
    rightCols.value = c;
    rightFields.value = leftFields.value.filter((f) => c.includes(f));
  });
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
  if (!leftKey.value || !leftFields.value.length) {
    message.info("请选择左文件及其主键字段");
    return;
  }
  if (!rightKey.value || !rightFields.value.length) {
    message.info("请选择右文件及其主键字段");
    return;
  }
  if (leftFields.value.length !== rightFields.value.length) {
    message.info("左右主键字段数量需一致");
    return;
  }
  running.value = true;
  cancelling.value = false;
  progress.value = null;
  result.value = null;
  try {
    const res = await invoke<DiffResult>("diff_files", {
      req: {
        left_key: leftKey.value,
        left_fields: leftFields.value,
        right_key: rightKey.value,
        right_fields: rightFields.value,
        output_name: outputName.value,
      },
    });
    result.value = res;
    doneKey.value = res.key;
    showDone.value = true;
    recordHistory("diff", `对账 ${res.key}`, `/files/preview?key=${res.key}`);
  } catch (e) {
    if (cancelling.value) {
      message.info("对账已取消");
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
    <h2>数据对账</h2>
    <p class="sub">
      两个文件按主键逐行比对，输出「仅左表 / 仅右表 / 有差异」的行并标注差异字段；相同行只计数不输出，结果生成新文件
    </p>

    <div class="card">
      <div class="section-title">左文件（基准）</div>
      <div class="row">
        <n-select
          :value="leftSource"
          class="file-select"
          :options="fileOptions"
          placeholder="选择左文件"
          :disabled="!fileOptions.length"
          @update:value="(s) => onLeftSource(s as string)"
        />
        <n-select
          :value="leftSheet"
          class="sheet-select"
          :options="sheetOptionsFor(leftSource)"
          :disabled="sheetOptionsFor(leftSource).length <= 1"
          placeholder="工作表"
          @update:value="(s) => onLeftSheet(s as string)"
        />
        <n-select
          :value="leftFields"
          multiple
          class="join-select"
          :options="leftCols.map((c) => ({ label: c, value: c }))"
          placeholder="主键字段（可多选）"
          :disabled="!leftCols.length"
          @update:value="onLeftFields"
        />
      </div>
    </div>

    <div class="card">
      <div class="section-title">右文件（对比）</div>
      <div class="row">
        <n-select
          :value="rightSource"
          class="file-select"
          :options="fileOptions"
          placeholder="选择右文件"
          :disabled="!fileOptions.length"
          @update:value="(s) => onRightSource(s as string)"
        />
        <n-select
          :value="rightSheet"
          class="sheet-select"
          :options="sheetOptionsFor(rightSource)"
          :disabled="sheetOptionsFor(rightSource).length <= 1"
          placeholder="工作表"
          @update:value="(s) => onRightSheet(s as string)"
        />
        <n-select
          v-model:value="rightFields"
          multiple
          class="join-select"
          :options="rightCols.map((c) => ({ label: c, value: c }))"
          placeholder="主键字段（按顺序对应左表）"
          :disabled="!rightCols.length"
        />
      </div>
      <p class="tip">右主键已按同名字段自动预填，可手动调整</p>
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
          开始对账
        </n-button>
      </div>
    </div>

    <div v-if="result" class="result-panel">
      <div class="result-head">
        <span class="result-title">对账完成</span>
      </div>
      <div class="result-meta">
        <span>仅左表：{{ result.only_left.toLocaleString() }}</span>
        <span>仅右表：{{ result.only_right.toLocaleString() }}</span>
        <span>有差异：{{ result.changed.toLocaleString() }}</span>
        <span>相同（未输出）：{{ result.same.toLocaleString() }}</span>
      </div>
      <div class="result-cols">新文件：{{ result.key }}</div>
    </div>

    <!-- 完成弹窗：确认跳转预览 -->
    <n-modal
      v-model:show="showDone"
      preset="dialog"
      title="对账完成"
      :positive-text="'去预览'"
      negative-text="留在本页"
      style="width: 420px"
      @positive-click="onGoPreview"
    >
      仅左表 {{ result?.only_left }} · 仅右表 {{ result?.only_right }} ·
      有差异 {{ result?.changed }} · 相同 {{ result?.same }}，是否前往预览页查看？
    </n-modal>

    <!-- 对账遮罩：全屏覆盖，期间无法操作其他页面 -->
    <div v-if="running" class="import-mask">
      <div class="mask-card">
        <n-spin size="large" />
        <p class="mask-title">
          {{
            progress?.stage === "read"
              ? `正在读取（${progress.sheet_index} / ${progress.sheet_count}）`
              : "正在对比…"
          }}
        </p>
        <p v-if="progress?.stage === 'read'" class="mask-sub">
          {{ progress.source }}
        </p>
        <n-progress
          v-if="progress?.stage === 'write' && progress.total"
          type="line"
          :percentage="progressPercent"
          :show-indicator="true"
          processing
          :height="8"
        />
        <p
          v-if="progress?.stage === 'write' && !progress.total"
          class="mask-sub"
        >
          已处理 {{ progress.processed.toLocaleString() }} 行…
        </p>
        <n-button secondary :loading="cancelling" @click="onCancel">
          {{ cancelling ? "正在取消…" : "取消对账" }}
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

.tip {
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 8px;
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
  flex-wrap: wrap;
  gap: 16px;
  font-size: 13px;
  color: var(--text);
  margin-bottom: 10px;
}

.result-cols {
  font-size: 13px;
  color: var(--text-muted);
}

/* 对账遮罩：全屏覆盖，禁止操作其他页面 */
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
