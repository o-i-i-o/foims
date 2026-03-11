pub fn get_backup_dir() -> String {
    if let Some(home) = std::env::var_os("HOME") {
        format!("{}/ipma_backups", home.to_string_lossy())
    } else {
        "/opt/ipma/backups".to_string()
    }
}

pub fn update_config_enabled(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = crate::config::get_config_file_path();
    let content = std::fs::read_to_string(&config_path)?;
    let mut value: toml::Value = toml::from_str(&content)?;
    if let Some(init) = value.get_mut("init")
        && let Some(table) = init.as_table_mut()
    {
        table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
    }
    let new_content = toml::to_string(&value)?;
    std::fs::write(&config_path, new_content)?;
    Ok(())
}
