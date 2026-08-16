//! 初始化功能配置（备份目录与启用开关）。

pub fn get_backup_dir() -> String {
    if let Some(home) = std::env::var_os("HOME") {
        format!("{}/ipma_backups", home.to_string_lossy())
    } else {
        "/opt/ipma/backups".to_string()
    }
}

pub async fn update_config_enabled(
    config_path: &str,
    enabled: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = tokio::fs::read_to_string(config_path).await?;
    let mut value: toml::Value = toml::from_str(&content)?;
    if let Some(init) = value.get_mut("init")
        && let Some(table) = init.as_table_mut()
    {
        table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
    }
    let new_content = toml::to_string(&value)?;
    tokio::fs::write(config_path, new_content).await?;
    Ok(())
}
