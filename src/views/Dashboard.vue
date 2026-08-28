<script setup lang="ts">
import WelcomeBanner from "@components/dashboard/WelcomeBanner.vue";
import QuickActions from "@components/dashboard/QuickActions.vue";
</script>

<template>
  <div class="dashboard">
    <WelcomeBanner />
    <QuickActions />
  </div>
</template>

<style scoped>
.dashboard {
  display: flex;
  flex-direction: column;
  gap: 24px;
  max-width: 960px;
  margin: 0 auto;
}
</style>
