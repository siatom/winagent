# WinAgent

**通过自然语言在 Windows 本机执行真实系统操作的桌面智能体。**

[English](README.md) | 简体中文

WinAgent 由 Rust 执行引擎、Tauri 2 桌面外壳和 React 界面组成。它连接兼容 OpenAI 协议的模型 API，在你的电脑上调用工具，再将结果回传给模型以继续完成任务。

> 命令会在真实的 Windows 系统上运行，没有执行沙箱。批准修改或删除数据的操作前，请检查审批内容。

## 功能

- 查看系统信息、管理进程、编辑环境变量、读取或修改注册表。
- 通过 PowerShell 处理文件、服务、网络、计划任务和 WMI/CIM，支持超时、取消及按需 UAC 提权。
- 流式展示模型回复、API 提供的推理内容和工具结果。
- 配置供应商、API 地址和模型列表，为每个会话选择模型。
- 使用 SQLite 保存会话和工具记录，通过 Windows 凭据管理器保存 API Key。
- 单实例运行，重复启动时聚焦已有窗口。

当前界面和内置系统提示词使用简体中文。

## 开始使用

按照下方步骤构建并启动应用，打开「设置 → 模型与供应商」，填写 Base URL、API Key 和模型 ID。应用会创建 DeepSeek 供应商，地址为 `https://api.deepseek.com`，默认模型为 `deepseek-flash`。这些是应用预设值，请按账号实际可用模型调整。

新建会话，例如：“列出内存占用最高的十个进程”或“检查 Windows 版本和可用磁盘空间”。执行过程中查看工具结果和审批请求。

### 模型兼容性

端点和模型需要支持流式 Chat Completions 和 function/tool calling。实际兼容性取决于供应商和模型。

Base URL 填写到主机地址或 `/v1`，不要填写完整的 `/chat/completions` 地址；客户端会在缺少 `/v1` 时自动追加。本地 HTTP 端点，例如 `http://localhost:1234/v1`，在服务端允许无认证访问时可不填 API Key。

## 权限与审批

应用以普通用户权限启动。显式要求提权的命令、系统级环境变量修改和 HKLM/HKCR 写入，可针对单次操作触发 UAC。

工具引擎依据 [risk.rs](crates/win-tools/src/risk.rs) 中的规则分类：

- `info` / `caution`：直接执行，并展示风险标签。
- `danger` / `critical`：暂停执行，展示命令、命中规则和后果，等待审批。
- 可在当前会话中记住同类操作的批准结果。

这是基于规则的审批机制，不是沙箱，也不能穷尽所有危险脚本。执行记录包含命令、风险等级、审批结果、退出状态、耗时和输出。

## 开发

环境要求：Windows、Microsoft Edge WebView2 Runtime、Rust **1.90+**（MSVC 工具链）、Visual Studio Build Tools（包含「使用 C++ 的桌面开发」和 Windows SDK）、Node.js **22.12+**、pnpm。

```powershell
git clone https://github.com/siatom/winagent.git
cd winagent
pnpm install
pnpm tauri dev
```

Vite 服务器使用 `1420` 端口，请确保端口可用。`pnpm dev` 只启动前端；需要桌面窗口和原生工具时使用 `pnpm tauri dev`。

构建 NSIS 安装包：

```powershell
pnpm build:local
```

默认产物位于工作区根目录的 `target/release/bundle/nsis/`。自定义 Cargo target 目录后，位置会随之改变。

构建并静默安装本地版本：

```powershell
pnpm install:local
```

请先关闭 WinAgent。命令会构建新的安装包并等待安装完成；构建失败时跳过安装，同时支持自定义 Cargo target 目录。`pnpm build` 仍然只构建前端。

验证改动：

```powershell
pnpm build
cargo test -p agent-core -p win-tools
```

## 项目结构

- `crates/agent-core/`：模型 API 客户端、流式响应、工具循环、审批与上下文裁剪。
- `crates/win-tools/`：PowerShell 执行器、Windows 工具与风险分类。
- `src-tauri/`：Tauri 外壳、SQLite、凭据、审批 broker 和 IPC。
- `src/`：React 19、TypeScript 和 Tailwind CSS 4 界面。

内置工具为 `run_powershell`、`process`、`env_var`、`registry` 和 `system_info`。其他系统管理任务统一通过 PowerShell 完成。

## 数据与隐私

- 会话、供应商设置和工具记录：`%APPDATA%\com.winagent.desktop\winagent.db`。
- API Key：Windows 凭据管理器，服务名为 `winagent`。已保存的密钥不写入 SQLite，也不返回前端。
- 执行临时文件：数据库同目录的 `runs/`，每次执行结束后清理对应的运行目录。
- 对话上下文和工具结果会发送给配置的模型端点。只有选择本地端点时，模型推理才在本地进行。

新数据库不存在时，WinAgent 会从旧版数据库导入快照；供应商凭据按需导入，原数据库保留。升级前请关闭旧版，之后使用新版，避免两份历史分别更新。

## 当前限制

- 系统管理工具面向 Windows。
- 每次 PowerShell 调用都会创建独立进程，变量、函数和 `Set-Location` 修改不跨调用保留；需要时显式指定工作目录，或将关联步骤合并到一段脚本中。
- 对话上下文按字符预算裁剪，没有向量记忆。
- 尚未实现多智能体编排、MCP 集成和插件市场。

## GitHub CI 与发布

推送到 `main` 或创建 Pull Request 时，CI 会在 Windows 上执行前端构建、Rust workspace 测试及桌面应用检查。

发布前，请同步 `package.json`、Cargo workspace 和 `src-tauri/tauri.conf.json` 的版本，再推送 `v<版本号>` 格式的 annotated tag（例如 `v0.1.0`）。Release 流程会在 Windows 上测试和构建，仅发布名为 `WinAgent_<版本号>_Windows_x64-setup.exe` 的 x64 NSIS 安装包。标签必须与配置版本一致。流程使用仓库的 `GITHUB_TOKEN`，无需额外配置发布 secret。
