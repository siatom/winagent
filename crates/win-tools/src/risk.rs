use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Info,
    Caution,
    Danger,
    Critical,
}

impl RiskLevel {
    pub fn requires_approval(self) -> bool {
        self >= RiskLevel::Danger
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RiskLevel::Info => "info",
            RiskLevel::Caution => "caution",
            RiskLevel::Danger => "danger",
            RiskLevel::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskHit {
    pub rule: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub reasons: Vec<RiskHit>,
    pub requires_approval: bool,
}

impl RiskAssessment {
    pub fn info() -> Self {
        Self { level: RiskLevel::Info, reasons: Vec::new(), requires_approval: false }
    }

    pub fn bump(&mut self, level: RiskLevel, rule: &str, reason: impl Into<String>) {
        if !self.reasons.iter().any(|r| r.rule == rule) {
            self.reasons.push(RiskHit { rule: rule.to_string(), reason: reason.into() });
        }
        if level > self.level {
            self.level = level;
        }
        self.requires_approval = self.level.requires_approval();
    }

    pub fn finish(mut self) -> Self {
        self.requires_approval = self.level.requires_approval();
        if self.level == RiskLevel::Info && self.reasons.is_empty() {
            self.reasons.push(RiskHit { rule: "read-only".into(), reason: "只读查询，无副作用".into() });
        }
        self
    }
}

struct Rule {
    id: &'static str,
    pattern: &'static str,
    level: RiskLevel,
    reason: &'static str,
}

const RULES: &[Rule] = &[
    Rule { id: "rm-recurse", pattern: r"remove-item\b[^\n;|]*-recurse", level: RiskLevel::Critical, reason: "递归删除文件或目录，删除内容通常不可恢复" },
    Rule { id: "rm-force", pattern: r"remove-item\b[^\n;|]*-force", level: RiskLevel::Caution, reason: "强制删除，跳过确认" },
    Rule { id: "rm", pattern: r"\bremove-item\b", level: RiskLevel::Caution, reason: "删除文件、目录或注册表项" },
    Rule { id: "rm-property", pattern: r"\bremove-itemproperty\b", level: RiskLevel::Caution, reason: "删除注册表值" },
    Rule { id: "del", pattern: r"\b(del|erase)\b", level: RiskLevel::Caution, reason: "删除文件" },
    Rule { id: "stop-process", pattern: r"\bstop-process\b", level: RiskLevel::Caution, reason: "结束进程" },
    Rule { id: "set-property", pattern: r"\b(set|new)-itemproperty\b", level: RiskLevel::Caution, reason: "修改注册表或配置项" },
    Rule { id: "rm-alias", pattern: r"\brm\b\s+-[a-z]*[rf]", level: RiskLevel::Caution, reason: "使用 rm 别名删除文件" },
    Rule { id: "del-silent", pattern: r"\b(del|erase)\b\s+/[a-z]*[sq]", level: RiskLevel::Danger, reason: "静默删除文件（/s 递归或 /q 免确认）" },
    Rule { id: "rd-recursive", pattern: r"\b(rd|rmdir)\b\s+/[a-z]*s", level: RiskLevel::Danger, reason: "递归删除目录" },
    Rule { id: "format", pattern: r"\bformat(-volume)?\b\s+[a-z]:", level: RiskLevel::Critical, reason: "格式化磁盘卷，数据将全部丢失" },
    Rule { id: "diskpart", pattern: r"\bdiskpart\b", level: RiskLevel::Critical, reason: "直接操作磁盘分区表" },
    Rule { id: "disk-admin", pattern: r"\b(clear-disk|initialize-disk|remove-partition|resize-partition|new-partition)\b", level: RiskLevel::Critical, reason: "磁盘/分区级操作" },
    Rule { id: "bcdedit", pattern: r"\bbcdedit\b", level: RiskLevel::Critical, reason: "修改启动引导配置，可能导致系统无法启动" },
    Rule { id: "vssadmin", pattern: r"\bvssadmin\b[^\n]*(delete|resize)", level: RiskLevel::Critical, reason: "删除/调整卷影副本，备份与还原点会失效" },
    Rule { id: "cipher-wipe", pattern: r"\bcipher\b\s+/w", level: RiskLevel::Danger, reason: "擦除磁盘空闲空间" },
    Rule { id: "reg-delete", pattern: r"\breg(\.exe)?\b\s+delete\b", level: RiskLevel::Danger, reason: "删除注册表键值" },
    Rule { id: "reg-hklm-write", pattern: r"\b(set|new)-itemproperty\b[^\n]*(hklm|hkey_local_machine)", level: RiskLevel::Critical, reason: "写入 HKLM 系统级注册表" },
    Rule { id: "reg-hklm-remove", pattern: r"\b(remove-item|remove-itemproperty)\b[^\n]*(hklm|hkey_local_machine)", level: RiskLevel::Critical, reason: "删除 HKLM 系统级注册表项" },
    Rule { id: "exec-policy", pattern: r"\bset-executionpolicy\b", level: RiskLevel::Danger, reason: "修改 PowerShell 脚本执行策略" },
    Rule { id: "sc", pattern: r"\bsc(\.exe)?\b\s+(delete|stop|config|create)\b", level: RiskLevel::Danger, reason: "修改 Windows 服务配置" },
    Rule { id: "service", pattern: r"\b(stop-service|set-service|new-service|remove-service|disable-service|restart-service)\b", level: RiskLevel::Caution, reason: "操作 Windows 服务" },
    Rule { id: "net-user", pattern: r"\b(net|net1)\b\s+user\b\s+\S+\s+\S+", level: RiskLevel::Danger, reason: "修改本地账户或密码" },
    Rule { id: "net-admin", pattern: r"\b(net|net1)\b\s+localgroup\b[^\n]*/add", level: RiskLevel::Critical, reason: "把账户加入本地组（可能提权为管理员）" },
    Rule { id: "acl", pattern: r"\b(takeown|icacls)\b", level: RiskLevel::Danger, reason: "修改文件/目录所有权或访问控制" },
    Rule { id: "power", pattern: r"\b(shutdown|restart-computer|stop-computer)\b", level: RiskLevel::Danger, reason: "关机或重启计算机" },
    Rule { id: "kill-force", pattern: r"\b(taskkill|stop-process)\b[^\n]*(/f|/t|-force)", level: RiskLevel::Caution, reason: "强制结束进程" },
    Rule { id: "iex", pattern: r"\b(invoke-expression|iex)\b", level: RiskLevel::Danger, reason: "动态执行字符串代码，内容无法静态审查" },
    Rule { id: "download", pattern: r"\b(downloadstring|downloadfile|invoke-webrequest|invoke-restmethod|start-bitstransfer|curl|wget)\b", level: RiskLevel::Caution, reason: "从网络下载内容" },
    Rule { id: "defender", pattern: r"\b(set|add|remove)-mppreference\b", level: RiskLevel::Critical, reason: "修改 Windows Defender 防护设置" },
    Rule { id: "firewall", pattern: r"\bnetsh\b\s+(advfirewall|firewall)", level: RiskLevel::Danger, reason: "修改防火墙规则" },
    Rule { id: "wmic-delete", pattern: r"\bwmic\b[^\n]*delete", level: RiskLevel::Danger, reason: "通过 WMI 删除系统对象" },
    Rule { id: "log-clear", pattern: r"\bwevtutil\b[^\n]*\bcl\b", level: RiskLevel::Danger, reason: "清空系统事件日志" },
    Rule { id: "user-create", pattern: r"\b(new-localuser|add-localgroupmember|enable-localuser)\b", level: RiskLevel::Caution, reason: "创建/启用本地账户或修改组成员" },
    Rule { id: "winrm", pattern: r"\b(enable-psremoting|set-item[^\n]*wsman|winrm\b)", level: RiskLevel::Danger, reason: "开启或修改远程管理能力" },
    Rule { id: "scheduled", pattern: r"\b(register-scheduledtask|schtasks)\b[^\n]*(create|/create)", level: RiskLevel::Caution, reason: "创建计划任务（可能用于持久化）" },
    Rule { id: "encoded", pattern: r"\s-(e|en|enc|encodedcommand)\b", level: RiskLevel::Danger, reason: "使用 -EncodedCommand 隐藏真实命令，无法静态审查" },
    Rule { id: "reg-hklm-add", pattern: r"\breg(\.exe)?\b\s+add\b[^\n]*(hklm|hkey_local_machine)", level: RiskLevel::Critical, reason: "写入 HKLM 系统级注册表" },
];

static COMPILED: LazyLock<Vec<(&'static Rule, Regex)>> = LazyLock::new(|| {
    RULES
        .iter()
        .filter_map(|r| match Regex::new(&format!("(?i){}", r.pattern)) {
            Ok(re) => Some((r, re)),
            Err(e) => {
                tracing::error!("risk rule {} failed to compile: {e}", r.id);
                None
            }
        })
        .collect()
});

/// Collapse common PowerShell obfuscation before matching.
pub fn normalize(script: &str) -> String {
    let lowered = script.to_lowercase();
    let stripped: String = lowered.chars().filter(|c| !matches!(c, '`' | '^')).collect();
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn classify_script(script: &str) -> RiskAssessment {
    let text = normalize(script);
    let mut assessment = RiskAssessment::info();
    for (rule, re) in COMPILED.iter() {
        if re.is_match(&text) {
            assessment.bump(rule.level, rule.id, rule.reason);
        }
    }
    assessment
}

const SYSTEM_CRITICAL: &[&str] = &[
    "lsass", "csrss", "wininit", "smss", "services", "winlogon", "system", "registry",
    "memory compression", "ntoskrnl",
];
const SYSTEM_HIGH: &[&str] = &["explorer", "svchost", "dwm", "audiodg", "msmpeng", "sihost", "ctfmon"];

/// Escalate a kill/stop operation that targets a critical Windows process.
pub fn assess_process_target(text: &str, assessment: &mut RiskAssessment) {
    let t = normalize(text);
    for name in SYSTEM_CRITICAL {
        if t.contains(name) {
            assessment.bump(
                RiskLevel::Critical,
                "system-process",
                format!("目标是系统关键进程 {name}，结束它会导致系统崩溃或注销"),
            );
        }
    }
    for name in SYSTEM_HIGH {
        if t.contains(name) {
            assessment.bump(
                RiskLevel::Danger,
                "shell-process",
                format!("目标是系统外壳/核心进程 {name}，结束它会造成桌面或服务异常"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_stays_info() {
        let a = classify_script("Get-Process | Select-Object -First 5");
        assert_eq!(a.level, RiskLevel::Info);
        assert!(!a.requires_approval);
    }

    #[test]
    fn recursive_delete_is_critical() {
        let a = classify_script("Remove-Item -Path 'C:\\temp' -Recurse -Force");
        assert_eq!(a.level, RiskLevel::Critical);
        assert!(a.requires_approval);
    }

    #[test]
    fn hklm_write_is_critical() {
        let a = classify_script("Set-ItemProperty -Path 'HKLM:\\SOFTWARE\\Test' -Name X -Value 1");
        assert_eq!(a.level, RiskLevel::Critical);
    }

    #[test]
    fn obfuscation_is_normalized() {
        let a = classify_script("Remove-Item  `\n -Recurse 'C:\\x'");
        assert!(a.requires_approval);
    }

    #[test]
    fn system_process_kill_is_critical() {
        let mut a = classify_script("Stop-Process -Name lsass -Force");
        assess_process_target("lsass", &mut a);
        assert_eq!(a.level, RiskLevel::Critical);
    }

    #[test]
    fn plain_kill_is_caution() {
        let a = classify_script("Stop-Process -Id 1234 -Force");
        assert_eq!(a.level, RiskLevel::Caution);
    }
}
