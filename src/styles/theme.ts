import type { GlobalThemeOverrides } from "naive-ui";

/** 青绿主色 + 大圆角，浅色模式 */
export const lightThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#10b981",
    primaryColorHover: "#34d399",
    primaryColorPressed: "#059669",
    primaryColorSuppl: "#34d399",
    borderRadius: "12px",
  },
  DataTable: {
    thTextColorWeight: "700",
    thColor: "rgba(15, 23, 42, 0.04)",
  },
};

/** 暗色模式用提亮变体，保证对比度 */
export const darkThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#34d399",
    primaryColorHover: "#6ee7b7",
    primaryColorPressed: "#10b981",
    primaryColorSuppl: "#6ee7b7",
    borderRadius: "12px",
  },
  DataTable: {
    thTextColorWeight: "700",
    thColor: "rgba(255, 255, 255, 0.04)",
  },
};
