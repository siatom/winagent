# WinAgent

**A Windows desktop agent that turns natural-language requests into real system actions.**

English | [简体中文](README.zh-CN.md)

WinAgent combines a Rust execution engine, a Tauri 2 desktop shell, and a React interface. It connects to an OpenAI-compatible model API, executes tools on your computer, and feeds their results back to the model to continue the task.

> Commands run on your actual Windows system without an execution sandbox. Review approval requests before allowing operations that modify or delete data.

## Features

- Inspect system information, manage processes, edit environment variables, and read or modify the registry.
- Run PowerShell scripts for files, services, networking, scheduled tasks, and WMI/CIM, with timeouts, cancellation, and on-demand UAC elevation.
- Stream responses, reasoning when supplied by the API, and tool results.
- Configure providers, endpoints, and model lists; select a model for each conversation.
- Save conversations and tool records in SQLite, and API keys through Windows Credential Manager.
- Run a single desktop instance; repeated launches focus the existing window.

The interface and built-in system prompt currently use Simplified Chinese.

## Getting started

Build and launch the app using the steps below, then open **Settings → Models & Providers** (`设置 → 模型与供应商`). Enter your Base URL, API key, and model ID. The app seeds a DeepSeek provider with `https://api.deepseek.com` and the default model `deepseek-flash`. These are application presets; select a model available to your account.

Start a conversation, for example: “Show the ten processes using the most memory” or “Check my Windows version and available disk space.” Review tool results and approval requests as the task progresses.

### Model compatibility

The endpoint and model must support streaming Chat Completions and function/tool calling. Actual compatibility depends on the provider and model.

Enter a base address ending at the host or `/v1`, rather than a full `/chat/completions` URL. The client appends `/v1` when absent. Local HTTP endpoints such as `http://localhost:1234/v1` can use an empty API key if the server allows unauthenticated requests.

## Permissions and approvals

WinAgent starts with standard user privileges. Explicitly elevated commands, system-level environment changes, and HKLM/HKCR writes can trigger UAC for individual operations.

Rules in [risk.rs](crates/win-tools/src/risk.rs) classify operations:

- `info` / `caution`: execute directly with a risk label.
- `danger` / `critical`: pause for approval and display the command, matching rules, and consequences.
- Approvals can be remembered for the same category within the current conversation.

This rule-based mechanism is not a sandbox or an exhaustive detector of dangerous scripts. Execution records include commands, risk levels, decisions, exit status, duration, and output.

## Development

Requirements: Windows, Microsoft Edge WebView2 Runtime, Rust **1.90+** with the MSVC toolchain, Visual Studio Build Tools with **Desktop development with C++** and a Windows SDK, Node.js **22.12+**, and pnpm.

```powershell
git clone https://github.com/siatom/winagent.git
cd winagent
pnpm install
pnpm tauri dev
```

The Vite server requires port `1420`. `pnpm dev` starts only the frontend; use `pnpm tauri dev` for the desktop app and native tools.

Build an NSIS installer:

```powershell
pnpm build:local
```

The default output is `target/release/bundle/nsis/` under the workspace root. A custom Cargo target directory changes this location.

Build and silently install the local version:

```powershell
pnpm install:local
```

Close WinAgent first. The command builds a fresh installer and waits for installation to finish; build failures skip installation. It also supports a custom Cargo target directory. `pnpm build` remains the frontend-only build.

Validate changes:

```powershell
pnpm build
cargo test -p agent-core -p win-tools
```

## Project structure

- `crates/agent-core/`: model API client, streaming, tool loop, approvals, and context trimming.
- `crates/win-tools/`: PowerShell runner, Windows tools, and risk classification.
- `src-tauri/`: Tauri shell, SQLite, credentials, approval broker, and IPC.
- `src/`: React 19, TypeScript, and Tailwind CSS 4 interface.

Built-in tools are `run_powershell`, `process`, `env_var`, `registry`, and `system_info`. Other administration tasks use PowerShell.

## Data and privacy

- Conversations, provider settings, and tool records: `%APPDATA%\com.winagent.desktop\winagent.db`.
- API keys: Windows Credential Manager, service name `winagent`. Saved keys are not stored in SQLite or returned to the frontend.
- Temporary execution files: `runs/` beside the database; each run's directory is cleaned up after execution.
- Conversation context and tool results are sent to your configured model endpoint. Model inference is local only when you configure a local endpoint.

If the new database is absent, WinAgent imports a snapshot of the previous installation's database. Provider credentials are imported on demand. The source database is preserved. Close the previous app before upgrading, and use the new version afterward to avoid diverging histories.

## Limitations

- Administration tools target Windows.
- Each PowerShell call starts a new process. Variables, functions, and `Set-Location` changes do not persist; use explicit working directories or combine steps in one script.
- Context is trimmed using a character budget, without vector-based memory.
- Multi-agent orchestration, MCP integration, and a plugin marketplace are not implemented.

## GitHub CI and releases

CI runs the frontend build, Rust workspace tests, and desktop checks on Windows for pushes to `main` and pull requests.

To publish a release, keep `package.json`, the Cargo workspace version, and `src-tauri/tauri.conf.json` in sync, then push an annotated `v<version>` tag (for example, `v0.1.0`). The Release workflow tests and builds on Windows, then publishes only the x64 NSIS installer named `WinAgent_<version>_Windows_x64-setup.exe`. Tags must match the configured version. The workflow uses the repository's `GITHUB_TOKEN`; no additional release secret is required.
