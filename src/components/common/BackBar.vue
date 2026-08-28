<script setup lang="ts">
import { useRouter } from "vue-router";
import { ArrowBackOutline } from "@vicons/ionicons5";

const router = useRouter();

/** 返回上一页；无历史记录时回到文件管理入口 */
function goBack() {
  if (window.history.length > 1) router.back();
  else router.push("/files");
}
</script>

<template>
  <div class="back-bar">
    <n-button text size="small" @click="goBack">
      <template #icon>
        <n-icon><ArrowBackOutline /></n-icon>
      </template>
      返回
    </n-button>
  </div>
</template>

<style scoped>
.back-bar {
  margin-bottom: 12px;
}
</style>
