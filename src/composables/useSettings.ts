import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

/** 全局设置，字段与 Rust 侧 AppSettings 一致（snake_case） */
export interface AppSettings {
  theme: "light" | "dark";
  page_size: number;
  /** 工作台快捷入口功能 key 列表；为空则全部显示 */
  quick_entries: string[];
  /** 导入分批行数：每批写入 parquet 的行数 */
  batch_rows: number;
}

const settings = ref<AppSettings>({
  theme: "light",
  page_size: 10,
  quick_entries: [],
  batch_rows: 8192,
});

/** 从磁盘加载配置；失败时保留默认值 */
export async function loadSettings() {
  try {
    const s = await invoke<AppSettings>("get_config");
    settings.value = { ...settings.value, ...s };
  } catch {
    /* 使用默认值 */
  }
}

/** 保存当前配置到磁盘 */
export async function saveSettings() {
  try {
    await invoke("save_config", { req: settings.value });
  } catch {
    /* 静默失败，下次变更仍会重新保存 */
  }
}

export function useSettings() {
  return { settings };
}
