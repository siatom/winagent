use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command as TokioCommand;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::types::ToolOutput;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const PRELUDE: &str = r#"
$ProgressPreference = 'SilentlyContinue'
$ErrorActionPreference = 'Continue'
try { [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false) } catch {}
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
"#;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("无法创建临时目录: {0}")]
    Io(#[from] std::io::Error),
    #[error("无法启动 PowerShell: {0}")]
    Spawn(String),
    #[error("用户取消了 UAC 提权")]
    ElevationDenied,
    #[error("{0}")]
    Other(String),
}

/// Quote a value for use inside a single-quoted PowerShell string literal.
pub fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Stateless PowerShell executor: one process per command, full output capture.
pub struct Shell {
    pwsh: PathBuf,
    temp_root: PathBuf,
}

impl Shell {
    pub fn detect(temp_root: PathBuf) -> Self {
        let pwsh = find_in_path("pwsh.exe")
            .or_else(|| {
                let fallback = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
                fallback.is_file().then_some(fallback)
            })
            .unwrap_or_else(|| PathBuf::from("powershell.exe"));
        Self { pwsh, temp_root }
    }

    pub fn pwsh_label(&self) -> String {
        self.pwsh
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "powershell".into())
    }

    pub async fn run(
        &self,
        script: &str,
        cwd: Option<&Path>,
        elevated: bool,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<ToolOutput, ShellError> {
        let dir = self.temp_root.join(format!("run-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await?;
        let result = if elevated {
            self.run_elevated(&dir, script, cwd, timeout, cancel).await
        } else {
            self.run_plain(&dir, script, cwd, timeout, cancel).await
        };
        let _ = tokio::fs::remove_dir_all(&dir).await;
        result
    }

    async fn write_script(&self, dir: &Path, name: &str, body: &str) -> std::io::Result<PathBuf> {
        let path = dir.join(name);
        // UTF-8 BOM keeps Windows PowerShell 5.1 from mangling non-ASCII scripts.
        tokio::fs::write(&path, format!("\u{feff}{body}")).await?;
        Ok(path)
    }

    async fn run_plain(
        &self,
        dir: &Path,
        script: &str,
        cwd: Option<&Path>,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<ToolOutput, ShellError> {
        let body = format!("{PRELUDE}\n{script}");
        let script_path = self.write_script(dir, "command.ps1", &body).await?;

        let mut cmd = TokioCommand::new(&self.pwsh);
        cmd.arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&script_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        let started = Instant::now();
        let mut child = cmd.spawn().map_err(|e| ShellError::Spawn(e.to_string()))?;
        let pid = child.id();
        let mut stdout_pipe = child.stdout.take().expect("stdout piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr piped");

        let out_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buf).await;
            buf
        });
        let err_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buf).await;
            buf
        });

        let mut timed_out = false;
        let mut cancelled = false;
        let status = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                cancelled = true;
                kill_tree(pid).await;
                let _ = child.kill().await;
                let _ = child.wait().await;
                None
            }
            _ = tokio::time::sleep(timeout) => {
                timed_out = true;
                kill_tree(pid).await;
                let _ = child.kill().await;
                let _ = child.wait().await;
                None
            }
            s = child.wait() => s.ok(),
        };

        let stdout = join_reader(out_task).await;
        let stderr = join_reader(err_task).await;
        let duration_ms = started.elapsed().as_millis() as u64;

        let mut stderr = stderr;
        if cancelled {
            stderr.push_str("\n[已取消：命令被用户中断]");
        } else if timed_out {
            stderr.push_str(&format!("\n[已超时：超过 {} 秒，进程树已终止]", timeout.as_secs()));
        }

        Ok(ToolOutput {
            ok: !timed_out && !cancelled && status.as_ref().and_then(|s| s.code()) == Some(0),
            exit_code: status.and_then(|s| s.code()),
            stdout,
            stderr,
            duration_ms,
            timed_out,
            truncated: false,
        })
    }

    /// Run the script through an elevated PowerShell (UAC prompt).
    async fn run_elevated(
        &self,
        dir: &Path,
        script: &str,
        cwd: Option<&Path>,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<ToolOutput, ShellError> {
        let payload = self.write_script(dir, "payload.ps1", &format!("{PRELUDE}\n{script}")).await?;
        let out_file = dir.join("stdout.txt");
        let err_file = dir.join("stderr.txt");
        let code_file = dir.join("exit.txt");

        let worker = format!(
            r#"$ErrorActionPreference = 'Stop'
try {{
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = {engine}
  $psi.Arguments = {args}
  $psi.WorkingDirectory = {cwd}
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.StandardOutputEncoding = [System.Text.UTF8Encoding]::new($false)
  $psi.StandardErrorEncoding = [System.Text.UTF8Encoding]::new($false)
  $psi.CreateNoWindow = $true
  $p = [System.Diagnostics.Process]::Start($psi)
  $out = $p.StandardOutput.ReadToEnd()
  $err = $p.StandardError.ReadToEnd()
  $p.WaitForExit()
  [IO.File]::WriteAllText({outf}, $out, [Text.UTF8Encoding]::new($false))
  [IO.File]::WriteAllText({errf}, $err, [Text.UTF8Encoding]::new($false))
  [IO.File]::WriteAllText({codef}, [string]$p.ExitCode, [Text.UTF8Encoding]::new($false))
}} catch {{
  [IO.File]::WriteAllText({errf}, $_.Exception.Message, [Text.UTF8Encoding]::new($false))
  [IO.File]::WriteAllText({codef}, 'FAILED', [Text.UTF8Encoding]::new($false))
}}
"#,
            engine = ps_quote(&self.pwsh.to_string_lossy()),
            args = ps_quote(&format!(
                "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\"",
                payload.display()
            )),
            cwd = ps_quote(&cwd.map(|p| p.display().to_string()).unwrap_or_else(|| {
                std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| "C:\\".into())
            })),
            outf = ps_quote(&out_file.to_string_lossy()),
            errf = ps_quote(&err_file.to_string_lossy()),
            codef = ps_quote(&code_file.to_string_lossy()),
        );
        let worker_path = self.write_script(dir, "worker.ps1", &worker).await?;

        let launcher = format!(
            r#"$ErrorActionPreference = 'Stop'
try {{
  Start-Process -FilePath {engine} -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',{worker}) -Verb RunAs -WindowStyle Hidden -Wait
}} catch {{
  [IO.File]::WriteAllText({codef}, 'DENIED', [Text.UTF8Encoding]::new($false))
  [IO.File]::WriteAllText({errf}, $_.Exception.Message, [Text.UTF8Encoding]::new($false))
}}
"#,
            engine = ps_quote(&self.pwsh.to_string_lossy()),
            worker = ps_quote(&format!("\"{}\"", worker_path.display())),
            codef = ps_quote(&code_file.to_string_lossy()),
            errf = ps_quote(&err_file.to_string_lossy()),
        );
        let launcher_path = self.write_script(dir, "launch.ps1", &launcher).await?;

        let started = Instant::now();
        let mut cmd = TokioCommand::new(&self.pwsh);
        cmd.arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&launcher_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(|e| ShellError::Spawn(e.to_string()))?;
        let pid = child.id();
        // Waiting for the user to answer a UAC prompt deserves extra patience.
        let effective_timeout = timeout + Duration::from_secs(120);
        let mut timed_out = false;
        let mut cancelled = false;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                cancelled = true;
                kill_tree(pid).await;
                let _ = child.kill().await;
            }
            _ = tokio::time::sleep(effective_timeout) => {
                timed_out = true;
                kill_tree(pid).await;
                let _ = child.kill().await;
            }
            _ = child.wait() => {}
        }
        let _ = child.wait().await;

        if cancelled {
            return Ok(ToolOutput {
                ok: false,
                exit_code: None,
                stdout: String::new(),
                stderr: "[已取消：提权命令被中断]".into(),
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: false,
                truncated: false,
            });
        }
        if timed_out {
            return Ok(ToolOutput {
                ok: false,
                exit_code: None,
                stdout: String::new(),
                stderr: "[已超时：等待提权执行超时，可能仍停留在 UAC 弹窗]".into(),
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: true,
                truncated: false,
            });
        }

        let code = read_text(&code_file).await;
        if code.trim() == "DENIED" {
            return Err(ShellError::ElevationDenied);
        }
        let stdout = read_text(&out_file).await;
        let stderr = read_text(&err_file).await;
        let exit_code = code.trim().parse::<i32>().ok();

        Ok(ToolOutput {
            ok: exit_code == Some(0),
            exit_code,
            stdout,
            stderr,
            duration_ms: started.elapsed().as_millis() as u64,
            timed_out: false,
            truncated: false,
        })
    }
}

async fn read_text(path: &Path) -> String {
    match tokio::fs::read(path).await {
        Ok(bytes) => strip_bom(&String::from_utf8_lossy(&bytes)),
        Err(_) => String::new(),
    }
}

fn strip_bom(s: &str) -> String {
    s.trim_start_matches('\u{feff}').to_string()
}

async fn join_reader(task: tokio::task::JoinHandle<Vec<u8>>) -> String {
    match tokio::time::timeout(Duration::from_secs(3), task).await {
        Ok(Ok(bytes)) => strip_bom(&String::from_utf8_lossy(&bytes)),
        _ => String::new(),
    }
}

async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    let _ = TokioCommand::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .await;
}

fn find_in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|p| p.join(exe)).find(|p| p.is_file())
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellInfo {
    pub pwsh: String,
    pub elevated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_stdout_stderr_and_exit_code() {
        let shell = Shell::detect(std::env::temp_dir().join("windows-god-test"));
        let out = shell
            .run(
                "Write-Output '你好 hello'\n[Console]::Error.WriteLine('oops')\nexit 3",
                None,
                false,
                Duration::from_secs(60),
                &CancellationToken::new(),
            )
            .await
            .expect("run");
        assert!(out.stdout.contains("hello"), "stdout was: {:?}", out.stdout);
        assert!(out.stderr.contains("oops"), "stderr was: {:?}", out.stderr);
        assert_eq!(out.exit_code, Some(3));
        assert!(!out.ok);
    }

    #[tokio::test]
    async fn timeout_kills_process_tree() {
        let shell = Shell::detect(std::env::temp_dir().join("windows-god-test"));
        let started = std::time::Instant::now();
        let out = shell
            .run(
                "Start-Sleep -Seconds 30",
                None,
                false,
                Duration::from_millis(700),
                &CancellationToken::new(),
            )
            .await
            .expect("run");
        assert!(out.timed_out);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn cancellation_stops_command() {
        let shell = Shell::detect(std::env::temp_dir().join("windows-god-test"));
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            token.cancel();
        });
        let out = shell
            .run("Start-Sleep -Seconds 30", None, false, Duration::from_secs(30), &cancel)
            .await
            .expect("run");
        assert!(out.stderr.contains("已取消"));
    }
}
