<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
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

const message = useMessage();
const router = useRouter();
const groups = ref<FileGroup[]>([]);

const selectedKeys = ref<string[]>([]);
const selectedFields = ref<string[]>([]);
const fieldOptions = ref<{ label: string; value: string }[]>([]);
const outputName = ref("合并结果");

const running = ref(false);
const cancelling = ref(false);
const progress = ref<ImportProgress | null>(null);
const resultRows = ref(0);
const resultKey = ref("");
const showDone = ref(false);
let unlistenProgress: UnlistenFn | undefined;

/** 平铺可选文件：label = 文件 / 工作表，value = parquet key */
const fileOptions = computed(() =>
  groups.value.flatMap((g) =>
    g.sheets.map((s) => ({
      label: `${g.source} / ${s.sheet || "默认"}`,
      value: s.key,
    }))
  )
);

const progressPercent = computed(() =>
  progress.value?.total
    ? Math.min(
        100,
        Math.round((progress.value.processed / progress.value.total) * 100)
      )
    : 0
);

async function loadCols(k: string): Promise<string[]> {
  try {
    return await invoke<string[]>("get_columns", { key: k });
  } catch {
    return [];
  }
}

/** 输出字段选项 = 选中文件的列并集（保序） */
async function refreshFieldOptions() {
  const seen = new Set<string>();
  const list: string[] = [];
  for (const k of selectedKeys.value) {
    for (const c of await loadCols(k)) {
      if (!seen.has(c)) {
        seen.add(c);
        list.push(c);
      }
    }
  }
  fieldOptions.value = list.map((c) => ({ label: c, value: c }));
  selectedFields.value = selectedFields.value.filter((f) => seen.has(f));
}

watch(selectedKeys, refreshFieldOptions);

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
  router.push({ path: "/files/preview", query: { key: resultKey.value } });
}

async function onRun() {
  if (selectedKeys.value.length < 2) {
    message.info("请至少选择两个文件");
    return;
  }
  running.value = true;
  cancelling.value = false;
  progress.value = null;
  resultKey.value = "";
  try {
    const res = await invoke<{ key: string; total: number }>("append_files", {
      req: {
        keys: selectedKeys.value,
        fields: selectedFields.value,
        output_name: outputName.value,
      },
    });
    resultRows.value = res.total;
    resultKey.value = res.key;
    showDone.value = true;
    recordHistory("append", `合并 ${res.key}`, `/files/preview?key=${res.key}`);
  } catch (e) {
    if (cancelling.value) {
      message.info("合并已取消");
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
    <h2>文件合并</h2>
    <p class="sub">
      多个文件按行堆叠成一个新文件，可选输出字段（不选 = 全部列并集，缺失列补空）；按选择顺序合并，原文件不变
    </p>

    <div class="card">
      <div class="section-title">
        选择文件
        <span class="hint">已选 {{ selectedKeys.length }} 个（至少 2 个）</span>
      </div>
      <div class="row">
        <n-select
          v-model:value="selectedKeys"
          multiple
          class="multi-select"
          :options="fileOptions"
          placeholder="选择要合并的文件（按选择顺序堆叠）"
          :disabled="!fileOptions.length"
        />
      </div>
      <div class="row">
        <n-select
          v-model:value="selectedFields"
          multiple
          class="multi-select"
          :options="fieldOptions"
          placeholder="输出字段（不选 = 全部列），按选择顺序输出"
          :disabled="!fieldOptions.length"
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
          执行合并
        </n-button>
      </div>
    </div>

    <div v-if="resultKey" class="result-panel">
      <div class="result-head">
        <span class="result-title">合并完成</span>
      </div>
      <div class="result-meta">
        <span>共 {{ resultRows.toLocaleString() }} 行</span>
      </div>
      <div class="result-cols">新文件：{{ resultKey }}</div>
    </div>

    <!-- 完成弹窗：确认跳转预览 -->
    <n-modal
      v-model:show="showDone"
      preset="dialog"
      title="合并完成"
      :positive-text="'去预览'"
      negative-text="留在本页"
      style="width: 400px"
      @positive-click="onGoPreview"
    >
      已合并 {{ resultRows.toLocaleString() }} 行（{{ resultKey
      }}），是否前往预览页查看？
    </n-modal>

    <!-- 合并遮罩：全屏覆盖，期间无法操作其他页面 -->
    <div v-if="running" class="import-mask">
      <div class="mask-card">
        <n-spin size="large" />
        <p class="mask-title">
          {{
            progress
              ? `正在合并（第 ${progress.sheet_index} / ${progress.sheet_count} 个文件）`
              : "正在准备…"
          }}
        </p>
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
          {{ cancelling ? "正在取消…" : "取消合并" }}
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

.hint {
  font-size: 12px;
  font-weight: 400;
  color: var(--text-muted);
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

.multi-select {
  width: 100%;
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

/* 合并遮罩：全屏覆盖，禁止操作其他页面 */
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
