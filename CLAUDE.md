# CLAUDE.md

## 概览

- Tauri 2 + Vue 3 + TypeScript + Vite 6 桌面应用（`rvt-file`）
- 前端 SPA 位于 `src/`，Rust 后端位于 `src-tauri/`。
- 包管理以bun为准（`tauri.conf.json` 的 `beforeDevCommand`/`beforeBuildCommand` 均用 `bun run`）；同时存在 `package-lock.json`，勿新增 npm 依赖造成锁文件不一致。
- 无测试框架、无 ESLint/Prettier 配置。类型检查由 `vue-tsc` 承担。

## 常用命令

| 命令 | 作用 |
|------|------|
| `bun run dev` | 仅启动 Vite 前端（端口 1420，strictPort） |
| `bun run tauri dev` | 启动完整桌面应用（先起前端再编译 Rust） |
| `bun run build` | 类型检查 + 前端构建（`vue-tsc --noEmit && vite build`） |
| `bun run tauri build` | 生产打包桌面应用 |

## 架构要点

- **前后端通信（IPC）**：前端 `import { invoke } from "@tauri-apps/api/core"` 调 `invoke("命令名", {...})` → Rust 侧 `#[tauri::command]` 函数，需在 `src-tauri/src/lib.rs` 的 `generate_handler!` 中注册。新增命令必须同时改两端。
- **Rust crate 命名**：lib 名为 `rvt_file_lib`（`_lib` 后缀是 Windows 下避免与 bin 名冲突所必需，勿改动），`main.rs` 仅调用 `rvt_file_lib::run()`。
- **权限模型**：`src-tauri/capabilities/default.json` 声明窗口（`main`）和插件权限。新增插件或系统能力时需在此添加对应权限，否则运行时会被拒。
- **Vite 配置**：dev server 端口固定 1420（HMR 用 1421），`vite.config.ts` 已配置忽略监听 `src-tauri`，前端热更新不触发 Rust 重编译。
- **桌面配置**：窗口标题、尺寸、图标、bundle 目标都在 `src-tauri/tauri.conf.json`；productName 与 `identifier` 均来自此文件。


## vue 组件创建

使用 `bun run generate` 创建 Vue 组件，模板位于 `plop-templates/component.hbs`。

示例：

```
bun run generate -- component --name TopTwo --dir src/components
```
