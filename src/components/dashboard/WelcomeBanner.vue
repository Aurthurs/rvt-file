<script setup lang="ts">
import { computed } from "vue";

const greeting = computed(() => {
  const hour = new Date().getHours();
  if (hour < 5) return "夜深了";
  if (hour < 12) return "早上好";
  if (hour < 18) return "下午好";
  return "晚上好";
});

const today = computed(() =>
  new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "long",
    day: "numeric",
    weekday: "long",
  }).format(new Date())
);
</script>

<template>
  <section class="banner">
    <div>
      <h1>{{ greeting }}，欢迎回来</h1>
      <p class="date">{{ today }}</p>
    </div>
    <p class="sub">一切尽在掌握，开始今天的工作吧</p>
  </section>
</template>

<style scoped>
.banner {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 24px;
  padding: 32px 36px;
  border-radius: var(--radius-lg);
  color: #fff;
  background: var(--brand-gradient);
  box-shadow: var(--shadow-brand);
}

h1 {
  font-size: 26px;
  font-weight: 700;
  margin-bottom: 8px;
}

.date {
  font-size: 14px;
  opacity: 0.92;
}

.sub {
  font-size: 14px;
  opacity: 0.95;
  flex-shrink: 0;
}
</style>
