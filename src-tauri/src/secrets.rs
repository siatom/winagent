const SERVICE: &str = "windows-god";

fn account(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

pub fn set_key(provider_id: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, &account(provider_id)).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_key(provider_id: &str) -> Option<String> {
    let entry = keyring::Entry::new(SERVICE, &account(provider_id)).ok()?;
    entry.get_password().ok().filter(|k| !k.trim().is_empty())
}

pub fn delete_key(provider_id: &str) {
    if let Ok(entry) = keyring::Entry::new(SERVICE, &account(provider_id)) {
        let _ = entry.delete_credential();
    }
}
