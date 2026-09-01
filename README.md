# RVT

轻量级表格文件管理桌面应用：上传 `xlsx / xls / csv` 文件，自动转换为 `parquet` 落盘，支持分页预览、多条件筛选、流式导出与文件融合。

<div align="center">
  <img src="images/dashboard.png" alt="工作台" width="80%">
</div>

## 功能

- **预览文件**：上传表格文件 → 转换 parquet → 表头 + 分页预览；支持多条件筛选，导出 CSV / Excel / Parquet（仅导出筛选后的数据）
- **文件融合**：合并多个 parquet 文件的字段与数据
- **质量检测**：字段统计与指标扫描
- **缓存管理**：查看 / 删除已转换的 parquet 缓存
- **设置**：亮暗主题切换（含 Windows 标题栏联动）、默认分页大小、导入分批行数、工作台快捷入口编辑

## 技术栈

- **桌面框架**：[Tauri 2](https://tauri.app)（Rust 后端）
- **前端**：Vue 3 + TypeScript + Vite 6 + [Naive UI](https://www.naiveui.com)
- **数据处理**：arrow / parquet / calamine / csv（Rust）
- **包管理**：bun（勿混用 npm 依赖，避免锁文件不一致）

## 环境要求

- Node.js ≥ 18
- [bun](https://bun.sh)
- Rust 工具链（stable）
- Windows 桌面开发依赖（MSVC Build Tools / WebView2）

## 开发

```bash
# 安装依赖
bun install

# 仅启动前端（端口 1420，HMR 1421）
bun run dev

# 启动完整桌面应用（前端 + Rust 编译）
bun run tauri dev
```

## 构建打包

```bash
# 类型检查 + 前端构建
bun run build

# 生产打包（NSIS / MSI，含中文安装包）
bunx tauri build
```

打包产物位于 `src-tauri/target/release/bundle/`。MSI 文件名格式：`RVT_<版本>_x64_zh-CN.msi`。

> 首次编译 arrow / parquet 依赖较慢（约 5-10 分钟），属正常现象。

## 项目结构

```
├── src/                  # 前端 SPA
│   ├── components/       # 通用组件 / 布局 / 工作台
│   ├── composables/      # 全局设置等组合式函数
│   ├── views/            # 页面（Dashboard / Files / Settings 等）
│   └── router/           # 路由
├── src-tauri/            # Rust 后端
│   ├── src/importer.rs   # 导入 / 转换 / 导出 / 设置 核心逻辑
│   ├── src/lib.rs        # command 注册
│   └── capabilities/     # 权限声明
├── plop-templates/       # 组件脚手架模板
└── vite.config.ts        # Vite 配置
```

## 数据存储

- 转换后的 parquet 文件存放在可执行文件同目录下的 `data/` 文件夹
- 全局设置（主题、分页、分批行数、快捷入口）持久化于应用配置目录的 `settings.json`

## 其他

- 新增 Vue 组件：`bun run generate`（Plop 脚手架）
- 大文件导入采用异步流式处理，支持中途取消，多 sheet 逐个处理、单个失败自动跳过
