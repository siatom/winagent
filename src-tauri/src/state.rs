use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::Manager;
use tokio_util::sync::CancellationToken;

use win_tools::{Engine, Shell};

use crate::approval::ApprovalBroker;
use crate::db::Db;

pub struct Dirs {
    pub data: PathBuf,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub engine: Arc<Engine>,
    pub shell: Arc<Shell>,
    pub broker: Arc<ApprovalBroker>,
    pub runs: Arc<Mutex<HashMap<String, RunHandle>>>,
    pub dirs: Arc<Dirs>,
    pub elevated: bool,
}

pub struct RunHandle {
    pub session_id: String,
    pub cancel: CancellationToken,
}

impl AppState {
    pub fn active_run_for(&self, session_id: &str) -> Option<String> {
        self.runs
            .lock()
            .ok()?
            .iter()
            .find(|(_, h)| h.session_id == session_id)
            .map(|(id, _)| id.clone())
    }
}

impl AppState {
    pub fn init(app: &tauri::AppHandle) -> Result<Self, String> {
        let data = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("无法定位应用数据目录: {e}"))?;
        let runs = data.join("runs");
        std::fs::create_dir_all(&runs).map_err(|e| format!("无法创建数据目录: {e}"))?;

        let db_path = data.join("winagent.db");
        if let Some(parent) = data.parent() {
            let legacy = parent.join("com.windowsgod.desktop").join("windows-god.db");
            crate::migration::import_database(&legacy, &db_path)?;
        }
        let db = Db::open(&db_path).map_err(|e| format!("无法打开数据库: {e}"))?;
        crate::migration::import_histories(&db, &parent_data_dir(&data))?;
        let shell = Arc::new(Shell::detect(runs.clone()));
        let engine = Arc::new(Engine::new(shell.clone()));

        Ok(Self {
            db: Arc::new(db),
            engine,
            shell,
            broker: Arc::new(ApprovalBroker::default()),
            runs: Arc::new(Mutex::new(HashMap::new())),
            dirs: Arc::new(Dirs { data }),
            elevated: is_elevated(),
        })
    }
}

fn parent_data_dir(data: &PathBuf) -> PathBuf {
    data.parent().map(PathBuf::from).unwrap_or_default()
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}
