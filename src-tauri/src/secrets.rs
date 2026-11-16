const SERVICE: &str = "winagent";

fn account(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

pub fn set_key(provider_id: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, &account(provider_id)).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_key(provider_id: &str) -> Option<String> {
    let entry = keyring::Entry::new(SERVICE, &account(provider_id)).ok()?;
    if let Some(key) = entry.get_password().ok().filter(|k| !k.trim().is_empty()) {
        return Some(key);
    }
    // Import a saved key on demand without deleting the original credential.
    let legacy = keyring::Entry::new("windows-god", &account(provider_id)).ok()?;
    let key = legacy.get_password().ok().filter(|k| !k.trim().is_empty())?;
    entry.set_password(&key).ok()?;
    Some(key)
}

pub fn delete_key(provider_id: &str) {
    if let Ok(entry) = keyring::Entry::new(SERVICE, &account(provider_id)) {
        let _ = entry.delete_credential();
    }
    // Prevent a removed key from being imported again on the next lookup.
    if let Ok(entry) = keyring::Entry::new("windows-god", &account(provider_id)) {
        let _ = entry.delete_credential();
    }
}
