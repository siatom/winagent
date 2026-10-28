use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::risk::{self, RiskAssessment, RiskLevel};
use crate::shell::{Shell, ShellError, ps_quote};
use crate::types::{ToolOutput, ToolSpec};

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "run_powershell",
            description: "在本机执行一段 PowerShell 脚本并返回退出码、stdout、stderr。这是最通用的工具：\
系统管理、文件操作、服务、网络、WMI/CIM、计划任务等都可以用它完成。每次调用是独立进程，\
工作目录会被记住，但变量和函数不会跨调用保留，需要时请在单次脚本内完成全部步骤。",
            parameters: json!({
                "type": "object",
                "properties": {
                    "script": { "type": "string", "description": "要执行的 PowerShell 脚本，可多行。不要包含 ``` 代码块标记。" },
                    "cwd": { "type": "string", "description": "工作目录，例如 D:\\\\projects。省略则沿用上一次的目录。" },
                    "timeout_sec": { "type": "integer", "minimum": 1, "maximum": 1800, "description": "超时秒数，默认 120。" },
                    "elevated": { "type": "boolean", "description": "是否以管理员权限运行（会弹出 UAC）。只有在确实需要时才设为 true。" }
                },
                "required": ["script"]
            }),
        },
        ToolSpec {
            name: "process",
            description: "查询或结束 Windows 进程。list 按内存占用排序返回进程表；kill 按 PID 或进程名结束进程。",
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "kill"], "description": "list 查询 / kill 结束" },
                    "filter": { "type": "string", "description": "list 时的进程名过滤（模糊匹配）" },
                    "pid": { "type": "integer", "description": "kill 时指定进程 PID" },
                    "name": { "type": "string", "description": "kill 时指定进程名，如 chrome（不含 .exe）" },
                    "force": { "type": "boolean", "description": "是否强制结束，默认 true" },
                    "elevated": { "type": "boolean", "description": "是否需要管理员权限，默认 false" }
                },
                "required": ["action"]
            }),
        },
        ToolSpec {
            name: "env_var",
            description: "读取、设置或删除环境变量。scope=user 写入当前用户，scope=machine 写入系统级（自动请求管理员权限），\
scope=process 只能读取当前进程。修改 PATH 时请务必先读取原值再追加，不要覆盖。",
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["get", "set", "delete"] },
                    "name": { "type": "string", "description": "变量名，省略时列出该作用域全部变量" },
                    "value": { "type": "string", "description": "set 时的值" },
                    "scope": { "type": "string", "enum": ["process", "user", "machine"], "description": "作用域，默认 user" }
                },
                "required": ["action"]
            }),
        },
        ToolSpec {
            name: "registry",
            description: "读写 Windows 注册表。hive 支持 HKCU/HKLM/HKCR/HKU。写 HKLM、HKCR、HKU 会自动请求管理员权限。\
修改前建议先用 action=get 或 list 确认现有值。",
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["get", "list", "set", "delete"] },
                    "hive": { "type": "string", "enum": ["HKCU", "HKLM", "HKCR", "HKU"], "description": "注册表根键" },
                    "path": { "type": "string", "description": "子路径，例如 SOFTWARE\\\\Microsoft\\\\Windows\\\\CurrentVersion" },
                    "name": { "type": "string", "description": "值名称；get/set/delete 单值时使用，省略则列出或删除整个键" },
                    "value": { "type": "string", "description": "set 时的值内容" },
                    "kind": { "type": "string", "enum": ["String", "ExpandString", "DWord", "QWord", "MultiString"], "description": "值类型，默认 String" }
                },
                "required": ["action", "hive", "path"]
            }),
        },
        ToolSpec {
            name: "system_info",
            description: "获取本机基础信息：系统版本、CPU、内存、磁盘、开机时间、当前是否管理员。只读。",
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

/// A tool call that has been parsed, classified and turned into a concrete script.
pub struct Prepared {
    pub tool: String,
    pub summary: String,
    pub script: String,
    pub elevated: bool,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    pub risk: RiskAssessment,
}

pub struct Engine {
    shell: Arc<Shell>,
}

impl Engine {
    pub fn new(shell: Arc<Shell>) -> Self {
        Self { shell }
    }

    pub fn prepare(&self, tool: &str, args: &Value) -> Result<Prepared, String> {
        match tool {
            "run_powershell" => prepare_powershell(args),
            "process" => prepare_process(args),
            "env_var" => prepare_env(args),
            "registry" => prepare_registry(args),
            "system_info" => Ok(system_info()),
            other => Err(format!("未知工具: {other}")),
        }
    }

    pub async fn execute(&self, prepared: &Prepared, cancel: &CancellationToken) -> ToolOutput {
        match self
            .shell
            .run(
                &prepared.script,
                prepared.cwd.as_deref(),
                prepared.elevated,
                prepared.timeout,
                cancel,
            )
            .await
        {
            Ok(mut out) => {
                if out.stdout.len() > 200_000 {
                    out.stdout.truncate(200_000);
                    out.truncated = true;
                }
                out
            }
            Err(ShellError::ElevationDenied) => {
                ToolOutput::failure("用户取消了 UAC 提权，命令未执行。请告知用户需要管理员权限，或改用不需要提权的方案。", 0)
            }
            Err(e) => ToolOutput::failure(e.to_string(), 0),
        }
    }
}

fn prepare_powershell(args: &Value) -> Result<Prepared, String> {
    let script = str_arg(args, "script").ok_or("缺少 script 参数")?;
    let elevated = bool_arg(args, "elevated", false);
    let timeout = Duration::from_secs(num_arg(args, "timeout_sec", 120).clamp(1, 1800) as u64);
    let cwd = str_arg(args, "cwd").map(PathBuf::from);
    let mut risk = risk::classify_script(&script);
    if elevated {
        risk.bump(RiskLevel::Caution, "elevated", "该命令将以管理员权限运行");
    }
    Ok(Prepared {
        tool: "run_powershell".into(),
        summary: first_line(&script),
        script,
        elevated,
        cwd,
        timeout,
        risk: risk.finish(),
    })
}

fn prepare_process(args: &Value) -> Result<Prepared, String> {
    let action = str_arg(args, "action").ok_or("缺少 action 参数")?;
    match action.as_str() {
        "list" => {
            let filter = str_arg(args, "filter");
            let where_clause = match &filter {
                Some(f) => format!("$_.ProcessName -like {}", ps_quote(&format!("*{f}*"))),
                None => "$true".to_string(),
            };
            let script = format!(
                r#"$items = Get-Process -ErrorAction SilentlyContinue |
  Where-Object {{ {where_clause} }} |
  Sort-Object -Property WorkingSet64 -Descending |
  Select-Object -First 60 Id, ProcessName,
    @{{n='MemMB';e={{[math]::Round($_.WorkingSet64/1MB,1)}}}},
    @{{n='CpuSec';e={{ if ($_.CPU) {{ [math]::Round($_.CPU,1) }} else {{ 0 }} }}}},
    Path
if (-not $items) {{ Write-Output '未找到匹配的进程'; exit 0 }}
$items | Format-Table -AutoSize | Out-String -Width 240"#
            );
            let summary = match &filter {
                Some(f) => format!("查询进程（过滤：{f}）"),
                None => "查询进程列表".into(),
            };
            Ok(Prepared {
                tool: "process".into(),
                summary,
                script,
                elevated: false,
                cwd: None,
                timeout: Duration::from_secs(60),
                risk: RiskAssessment::info().finish(),
            })
        }
        "kill" => {
            let pid = args.get("pid").and_then(|v| v.as_i64());
            let name = str_arg(args, "name");
            if pid.is_none() && name.is_none() {
                return Err("kill 需要 pid 或 name".into());
            }
            let force = bool_arg(args, "force", true);
            let elevated = bool_arg(args, "elevated", false);
            let selector = if let Some(pid) = pid {
                format!("$_.Id -eq {pid}")
            } else {
                format!("$_.ProcessName -eq {}", ps_quote(name.as_deref().unwrap_or_default()))
            };
            let force_arg = if force { " -Force" } else { "" };
            let script = format!(
                r#"$targets = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {{ {selector} }})
if ($targets.Count -eq 0) {{ Write-Output '未找到指定的进程'; exit 1 }}
foreach ($p in $targets) {{
  try {{
    Stop-Process -Id $p.Id{force_arg} -ErrorAction Stop
    Write-Output ("已结束 {{0}} (PID {{1}})" -f $p.ProcessName, $p.Id)
  }} catch {{
    Write-Error ("结束 {{0}} (PID {{1}}) 失败: {{2}}" -f $p.ProcessName, $p.Id, $_.Exception.Message)
  }}
}}"#
            );
            let target = pid.map(|p| format!("PID {p}")).unwrap_or_else(|| name.clone().unwrap_or_default());
            let mut risk = risk::RiskAssessment::info();
            risk.bump(RiskLevel::Caution, "kill-process", format!("结束进程 {target}"));
            if force {
                risk.bump(RiskLevel::Caution, "kill-force", "强制结束进程，可能丢失未保存数据");
            }
            risk::assess_process_target(&target, &mut risk);
            Ok(Prepared {
                tool: "process".into(),
                summary: format!("结束进程 {target}"),
                script,
                elevated,
                cwd: None,
                timeout: Duration::from_secs(60),
                risk: risk.finish(),
            })
        }
        other => Err(format!("process 不支持 action={other}")),
    }
}

fn prepare_env(args: &Value) -> Result<Prepared, String> {
    let action = str_arg(args, "action").ok_or("缺少 action 参数")?;
    let name = str_arg(args, "name");
    let scope = str_arg(args, "scope").unwrap_or_else(|| "user".into());
    let upper = match scope.as_str() {
        "process" | "user" | "machine" => {
            let mut c = scope.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or(scope.clone())
        }
        other => return Err(format!("scope 不支持 {other}")),
    };
    let machine = scope == "machine";
    let elevated = machine && action != "get";

    let mut risk = RiskAssessment::info();
    let (script, summary) = match action.as_str() {
        "get" => {
            let script = match &name {
                Some(n) => format!("[Environment]::GetEnvironmentVariable({}, {})", ps_quote(n), ps_quote(&upper)),
                None => format!(
                    "[Environment]::GetEnvironmentVariables({}).GetEnumerator() | Sort-Object Name | \
ForEach-Object {{ '{{0}}={{1}}' -f $_.Key, $_.Value }}",
                    ps_quote(&upper)
                ),
            };
            (script, name.map(|n| format!("读取环境变量 {n}")).unwrap_or_else(|| "列出环境变量".into()))
        }
        "set" => {
            let n = name.ok_or("set 需要 name")?;
            let v = str_arg(args, "value").unwrap_or_default();
            let name_q = ps_quote(&n);
            let value_q = ps_quote(&v);
            let scope_q = ps_quote(&upper);
            let script = format!(
                "[Environment]::SetEnvironmentVariable({name_q}, {value_q}, {scope_q}); \
'已设置 {n} = ' + [Environment]::GetEnvironmentVariable({name_q}, {scope_q})"
            );
            risk.bump(RiskLevel::Caution, "env-set", format!("修改 {upper} 级环境变量 {n}"));
            if n.eq_ignore_ascii_case("PATH") {
                risk.bump(
                    if machine { RiskLevel::Critical } else { RiskLevel::Danger },
                    "env-path",
                    "修改 PATH 可能影响系统或用户命令查找，写错会导致程序无法启动",
                );
            }
            if machine {
                risk.bump(RiskLevel::Danger, "env-machine", "修改系统级环境变量会影响所有用户，并需要管理员权限");
            }
            (script, format!("设置环境变量 {n}"))
        }
        "delete" => {
            let n = name.ok_or("delete 需要 name")?;
            if scope == "process" {
                return Err("无法在独立进程中删除环境变量，请使用 user 或 machine".into());
            }
            let script = format!(
                "[Environment]::SetEnvironmentVariable({}, $null, {}); '已删除 {n}'",
                ps_quote(&n),
                ps_quote(&upper)
            );
            risk.bump(
                if machine { RiskLevel::Critical } else { RiskLevel::Danger },
                "env-delete",
                format!("删除 {upper} 级环境变量 {n}"),
            );
            (script, format!("删除环境变量 {n}"))
        }
        other => return Err(format!("env_var 不支持 action={other}")),
    };

    Ok(Prepared {
        tool: "env_var".into(),
        summary,
        script,
        elevated,
        cwd: None,
        timeout: Duration::from_secs(60),
        risk: risk.finish(),
    })
}

fn prepare_registry(args: &Value) -> Result<Prepared, String> {
    let action = str_arg(args, "action").ok_or("缺少 action 参数")?;
    let hive = str_arg(args, "hive").ok_or("缺少 hive 参数")?;
    let path = str_arg(args, "path").ok_or("缺少 path 参数")?;
    let name = str_arg(args, "name");

    let hive_long = match hive.to_uppercase().as_str() {
        "HKCU" | "HKEY_CURRENT_USER" => "HKEY_CURRENT_USER",
        "HKLM" | "HKEY_LOCAL_MACHINE" => "HKEY_LOCAL_MACHINE",
        "HKCR" | "HKEY_CLASSES_ROOT" => "HKEY_CLASSES_ROOT",
        "HKU" | "HKEY_USERS" => "HKEY_USERS",
        other => return Err(format!("不支持的 hive: {other}")),
    };
    let sub = path.trim_start_matches('\\');
    let full_key = if sub.is_empty() { hive_long.to_string() } else { format!("{hive_long}\\{sub}") };
    let provider = format!("Registry::{}", full_key);
    let is_hkcu = hive_long == "HKEY_CURRENT_USER";
    let elevated = !is_hkcu && matches!(action.as_str(), "set" | "delete");

    let mut risk = RiskAssessment::info();
    let (script, summary) = match action.as_str() {
        "get" => {
            let script = match &name {
                Some(n) => format!(
                    "$item = Get-Item -LiteralPath {} -ErrorAction Stop; \
$v = $item.GetValue({}, $null); \
if ($null -eq $v) {{ Write-Output '(值不存在)'; exit 1 }}; \
'{{0}} = {{1}} ({{2}})' -f {}, ($v -join ', '), $item.GetValueKind({})",
                    ps_quote(&provider),
                    ps_quote(n),
                    ps_quote(n),
                    ps_quote(n)
                ),
                None => format!(
                    "$item = Get-Item -LiteralPath {} -ErrorAction Stop; \
'--- 子键 ---'; \
Get-ChildItem -LiteralPath {} -ErrorAction SilentlyContinue | ForEach-Object {{ $_.PSChildName }}; \
'--- 值 ---'; \
$item.GetValueNames() | ForEach-Object {{ '{{0}} = {{1}} ({{2}})' -f $_, ($item.GetValue($_) -join ', '), $item.GetValueKind($_) }}",
                    ps_quote(&provider),
                    ps_quote(&provider)
                ),
            };
            (script, format!("读取注册表 {full_key}"))
        }
        "list" => {
            let script = format!(
                "'--- 子键 ---'; \
Get-ChildItem -LiteralPath {} -ErrorAction SilentlyContinue | ForEach-Object {{ $_.PSChildName }}; \
'--- 值 ---'; \
$item = Get-Item -LiteralPath {} -ErrorAction Stop; \
$item.GetValueNames() | ForEach-Object {{ '{{0}} = {{1}} ({{2}})' -f $_, ($item.GetValue($_) -join ', '), $item.GetValueKind($_) }}",
                ps_quote(&provider),
                ps_quote(&provider)
            );
            (script, format!("列出注册表 {full_key}"))
        }
        "set" => {
            let n = name.ok_or("set 需要 name")?;
            let v = str_arg(args, "value").unwrap_or_default();
            let kind = str_arg(args, "kind").unwrap_or_else(|| "String".into());
            let value_expr = match kind.as_str() {
                "DWord" | "QWord" => format!("[int64]{}", v.trim()),
                _ => ps_quote(&v),
            };
            let kind_final = match kind.as_str() {
                "String" | "ExpandString" | "DWord" | "QWord" | "MultiString" => kind.clone(),
                _ => "String".into(),
            };
            let script = format!(
                "if (-not (Test-Path -LiteralPath {})) {{ New-Item -Path {} -Force | Out-Null }}; \
New-ItemProperty -LiteralPath {} -Name {} -Value {} -PropertyType {} -Force | Out-Null; \
'已写入 {n} (类型 {kind_final})'",
                ps_quote(&provider),
                ps_quote(&provider),
                ps_quote(&provider),
                ps_quote(&n),
                value_expr,
                kind_final
            );
            if is_hkcu {
                risk.bump(RiskLevel::Caution, "reg-write", "写入当前用户注册表");
            } else {
                risk.bump(RiskLevel::Critical, "reg-write-system", "写入系统级注册表（HKLM/HKCR/HKU）");
            }
            (script, format!("写入注册表 {full_key} → {n}"))
        }
        "delete" => {
            let script = match &name {
                Some(n) => format!(
                    "Remove-ItemProperty -LiteralPath {} -Name {} -Force -ErrorAction Stop; '已删除值 {n}'",
                    ps_quote(&provider),
                    ps_quote(n)
                ),
                None => format!(
                    "Remove-Item -LiteralPath {} -Recurse -Force -ErrorAction Stop; '已删除键 {full_key}'",
                    ps_quote(&provider)
                ),
            };
            risk.bump(
                if is_hkcu { RiskLevel::Danger } else { RiskLevel::Critical },
                "reg-delete",
                "删除注册表内容，可能导致程序或系统功能异常",
            );
            (script, format!("删除注册表 {full_key}"))
        }
        other => return Err(format!("registry 不支持 action={other}")),
    };

    Ok(Prepared {
        tool: "registry".into(),
        summary,
        script,
        elevated,
        cwd: None,
        timeout: Duration::from_secs(60),
        risk: risk.finish(),
    })
}

fn system_info() -> Prepared {
    let script = r#"$os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
$cs = Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue
$cpu = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
"主机名: $env:COMPUTERNAME"
"当前用户: $env:USERNAME"
"系统: $($os.Caption) $($os.Version) (Build $($os.BuildNumber))"
"架构: $($os.OSArchitecture)"
"CPU: $($cpu.Name) ($($cpu.NumberOfCores) 核 / $($cpu.NumberOfLogicalProcessors) 线程)"
"内存: 总计 $([math]::Round($cs.TotalPhysicalMemory/1GB,1)) GB, 可用 $([math]::Round($os.FreePhysicalMemory/1MB,1)) GB"
"开机时间: $($os.LastBootUpTime)"
"管理员权限: $isAdmin"
"PowerShell: $($PSVersionTable.PSVersion)"
"磁盘:"
Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' -ErrorAction SilentlyContinue | ForEach-Object {
  "  $($_.DeviceID) 可用 $([math]::Round($_.FreeSpace/1GB,1)) GB / 总计 $([math]::Round($_.Size/1GB,1)) GB"
}"#;
    Prepared {
        tool: "system_info".into(),
        summary: "读取系统信息".into(),
        script: script.into(),
        elevated: false,
        cwd: None,
        timeout: Duration::from_secs(60),
        risk: RiskAssessment::info().finish(),
    }
}

fn first_line(script: &str) -> String {
    let line = script.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("PowerShell 命令");
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(80).collect::<String>())
    } else {
        line.to_string()
    }
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn num_arg(args: &Value, key: &str, default: i64) -> i64 {
    args.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_powershell_requires_script() {
        assert!(prepare_powershell(&json!({})).is_err());
    }

    #[test]
    fn registry_hklm_marks_critical_and_elevates() {
        let p = prepare_registry(&json!({
            "action": "set", "hive": "HKLM", "path": "SOFTWARE\\Test", "name": "X", "value": "1"
        }))
        .unwrap();
        assert!(p.elevated);
        assert_eq!(p.risk.level, RiskLevel::Critical);
    }

    #[test]
    fn registry_hkcu_read_is_info() {
        let p = prepare_registry(&json!({
            "action": "get", "hive": "HKCU", "path": "Environment", "name": "Path"
        }))
        .unwrap();
        assert!(!p.elevated);
        assert_eq!(p.risk.level, RiskLevel::Info);
    }

    #[test]
    fn machine_env_elevates() {
        let p = prepare_env(&json!({
            "action": "set", "name": "FOO", "value": "bar", "scope": "machine"
        }))
        .unwrap();
        assert!(p.elevated);
        assert!(p.risk.requires_approval);
    }

    #[test]
    fn all_specs_serialize() {
        for spec in tool_specs() {
            serde_json::to_value(spec).unwrap();
        }
    }
}
