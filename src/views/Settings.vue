<script setup lang="ts">
import { onMounted } from "vue";
import { useMessage } from "naive-ui";
import BackBar from "@components/common/BackBar.vue";
import { loadSettings, saveSettings, useSettings } from "@/composables/useSettings";

const message = useMessage();
const { settings } = useSettings();

const pageSizeOptions = [10, 20, 50, 100].map((n) => ({
  label: `${n} 条/页`,
  value: n,
}));

const batchRowsOptions = [4096, 8192, 16384, 32768].map((n) => ({
  label: `${n} 行/批`,
  value: n,
}));

onMounted(loadSettings);

/** 主题开关：开 = 暗色，关 = 浅色 */
async function onThemeChange(v: boolean) {
  settings.value.theme = v ? "dark" : "light";
  await saveSettings();
  message.success(v ? "已切换为暗色主题" : "已切换为浅色主题");
}

async function onPageSizeChange(v: number) {
  settings.value.page_size = v;
  await saveSettings();
  message.success(`默认分页大小已设为 ${v} 条`);
}

async function onBatchRowsChange(v: number) {
  settings.value.batch_rows = v;
  await saveSettings();
  message.success(`导入分批行数已设为 ${v} 行`);
}
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>设置</h2>
    <p class="sub">全局偏好设置，修改后自动保存。</p>

    <div class="card">
      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">暗色主题</div>
          <div class="setting-desc">关闭为浅色模式，开启为暗色模式</div>
        </div>
        <n-switch
          :value="settings.theme === 'dark'"
          @update:value="onThemeChange"
        />
      </div>
      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">默认分页大小</div>
          <div class="setting-desc">预览与缓存表格每页展示的数据条数</div>
        </div>
        <n-select
          :value="settings.page_size"
          class="size-select"
          :options="pageSizeOptions"
          @update:value="(v) => onPageSizeChange(v as number)"
        />
      </div>
      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">导入分批行数</div>
          <div class="setting-desc">
            导入文件时分批写入的行数，数值越小内存占用越低、速度略慢
          </div>
        </div>
        <n-select
          :value="settings.batch_rows"
          class="size-select"
          :options="batchRowsOptions"
          @update:value="(v) => onBatchRowsChange(v as number)"
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
  padding: 8px 20px;
  background: var(--surface);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
}

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 0;
  border-bottom: 1px solid var(--border);
}

.setting-row:last-child {
  border-bottom: none;
}

.setting-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.setting-desc {
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 2px;
}

.size-select {
  width: 140px;
}
</style>
