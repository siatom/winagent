# Winctor

运行在本机的 Windows 超级管理员智能体。Rust 内核 + Tauri 2 桌面外壳，模型通过 OpenAI 兼容协议接入（默认 DeepSeek）。

不是聊天玩具：模型输出的每一条命令都会在本机真实执行，并把真实的 stdout / stderr / 退出码回灌给模型继续推理。

## 能力

| 工具 | 说明 |
| --- | --- |
| `run_powershell` | 任意 PowerShell 脚本，支持超时、工作目录记忆、按需 UAC 提权 |
| `process` | 查询进程（按内存排序）、按 PID 或进程名结束进程 |
| `env_var` | 读取 / 设置 / 删除用户级与系统级环境变量 |
| `registry` | 读取 / 写入 / 删除注册表（HKCU / HKLM / HKCR / HKU） |
| `system_info` | 系统版本、CPU、内存、磁盘、开机时间、当前权限 |

其余系统管理能力（服务、网络、防火墙、计划任务、WMI）统一走 `run_powershell`，不堆砌工具。

## 风险模型：提醒，而不是限制

默认没有白名单、没有黑名单、没有沙箱。危险操作只提醒，不禁止：

- `info` / `caution`：直接执行，在消息里打风险标签
- `danger` / `critical`：暂停执行，弹窗展示命令原文、命中规则和后果，确认后才运行
- 弹窗可勾选「本次会话内，同类操作不再询问」
- 每条命令的完整记录（命令、风险等级、审批结果、退出码、输出）写入本地 `tool_runs` 表

风险规则见 `crates/win-tools/src/risk.rs`，覆盖递归删除、格式化、diskpart、bcdedit、HKLM 写入、Defender 配置、服务控制、账户提权、强杀系统进程、`-EncodedCommand` 混淆等 30+ 条。

## 架构

```
crates/agent-core/    与 Tauri 无关的内核
  provider.rs         OpenAI 兼容 SSE 流式客户端（文本 / reasoning / tool_calls）
  runner.rs           工具调用循环、审批挂起、取消、上下文裁剪
  message.rs          消息模型、历史清洗
  prompt.rs           系统提示词
crates/win-tools/     Windows 能力层
  shell.rs            PowerShell 执行器（进程树终止、UAC 提权）
  tools.rs            工具定义与脚本生成
  risk.rs             风险分类
src-tauri/            桌面壳：SQLite、凭据管理器、审批 broker、IPC Channel
src/                  React 19 + Tailwind 4 前端
```

## 开发

环境要求：Rust 1.90+（MSVC）、Node 20+、pnpm、WebView2（Windows 11 自带）。

```bash
pnpm install
pnpm tauri dev
```

构建安装包：

```bash
pnpm tauri build
# 产物：src-tauri/target/release/bundle/nsis/
```

测试：

```bash
cargo test -p agent-core -p win-tools
pnpm build
```

## 配置

首次启动会自动创建 DeepSeek 供应商：

- Base URL：`https://api.deepseek.com`
- 默认模型：`deepseek-flash`
- 可选模型：`deepseek-chat`、`deepseek-reasoner`

在「设置 → 模型与供应商」填入 API Key 即可使用。任何 OpenAI 兼容端点都能接入（OpenAI、Kimi、智谱、硅基流动、OpenRouter、Ollama、vLLM、LM Studio），Base URL 填到域名或 `/v1`。

本地端点（`127.0.0.1` / `localhost`）不需要 API Key，留空即可。

API Key 保存在 Windows 凭据管理器（服务名 `windows-god`），不写入数据库，也不会返回给前端。

## 权限

应用以标准权限启动，`elevated` 命令、系统级环境变量、HKLM/HKCR 写入会自动通过 UAC 单条提权，不需要整个程序常驻管理员。

## 数据

- SQLite：`%APPDATA%\com.windowsgod.desktop\windows-god.db`
- 运行临时脚本：同目录 `runs/`，执行完即删

## 已知限制

- 每条 PowerShell 是独立进程：工作目录会记住，但变量、函数、`Set-Location` 不跨调用保留
- 上下文按字符预算裁剪，不做向量记忆
- 未实现多智能体、MCP、插件市场
