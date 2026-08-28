<script setup lang="ts">
import BackBar from "@components/common/BackBar.vue";
</script>

<template>
  <div class="page">
    <BackBar />
    <h2>质量检测</h2>
    <p class="sub">检查文件完整性与质量。</p>
    <div class="placeholder">
      <n-empty description="功能开发中，敬请期待" />
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

.placeholder {
  margin-top: 40px;
}
</style>
