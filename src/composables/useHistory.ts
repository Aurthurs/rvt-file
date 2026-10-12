import { invoke } from "@tauri-apps/api/core";

/** 操作类型 -> 中文标签 */
export const actionLabels: Record<string, string> = {
  import: "导入",
  export: "导出",
  merge: "融合",
  translate: "翻译",
  append: "合并",
  diff: "对账",
  dedupe: "去重",
  aggregate: "汇总",
  transform: "变换",
  /** 有历史记录前的旧产物（manifest 中 source==key），来源细分不可考 */
  processed: "处理产出",
  /** manifest 中也没有的外部 parquet */
  other: "其他",
};

/** 记录一条操作历史（失败静默，不影响主流程） */
export async function recordHistory(
  action: string,
  title: string,
  detail: string
) {
  try {
    await invoke("push_history", { req: { action, title, detail } });
  } catch {
    /* 历史记录失败可忽略 */
  }
}
