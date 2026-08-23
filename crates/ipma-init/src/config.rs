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

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建进程唯一的临时目录，返回其路径（测试结束后由调用方清理）
    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ipma_init_config_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("创建临时目录失败: {e}"));
        dir
    }

    #[test]
    fn get_backup_dir_跟随home环境变量() {
        let dir = get_backup_dir();
        if let Some(home) = std::env::var_os("HOME") {
            let expected = format!("{}/ipma_backups", home.to_string_lossy());
            assert_eq!(dir, expected);
        } else {
            assert_eq!(dir, "/opt/ipma/backups");
        }
    }

    /// 更新 enabled 开关并保留同表其他键
    #[tokio::test]
    async fn update_config_enabled_修改开关并保留其他键() {
        let dir = temp_dir();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[init]\nenabled = false\nname = \"ipma\"\n")
            .unwrap_or_else(|e| panic!("写入测试配置失败: {e}"));

        let path_str = path.to_string_lossy().to_string();
        update_config_enabled(&path_str, true)
            .await
            .unwrap_or_else(|e| panic!("更新配置失败: {e}"));

        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取配置失败: {e}"));
        let value: toml::Value = toml::from_str(&content).unwrap_or_else(|e| {
            panic!("解析更新后的配置失败: {e}");
        });
        let init = value
            .get("init")
            .unwrap_or_else(|| panic!("应保留 init 表"));
        assert_eq!(
            init.get("enabled").and_then(toml::Value::as_bool),
            Some(true)
        );
        assert_eq!(init.get("name").and_then(toml::Value::as_str), Some("ipma"));

        // 再切回 false 验证双向生效
        update_config_enabled(&path_str, false)
            .await
            .unwrap_or_else(|e| panic!("更新配置失败: {e}"));
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取配置失败: {e}"));
        let value: toml::Value =
            toml::from_str(&content).unwrap_or_else(|e| panic!("解析配置失败: {e}"));
        let init = value
            .get("init")
            .unwrap_or_else(|| panic!("应保留 init 表"));
        assert_eq!(
            init.get("enabled").and_then(toml::Value::as_bool),
            Some(false)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 缺少 init 表时静默不修改（记录现有行为：不报错也不新增表）
    #[tokio::test]
    async fn update_config_enabled_缺少init表时不新增() {
        let dir = temp_dir();
        let path = dir.join("no_init.toml");
        std::fs::write(&path, "[server]\nport = 8080\n")
            .unwrap_or_else(|e| panic!("写入测试配置失败: {e}"));

        let path_str = path.to_string_lossy().to_string();
        update_config_enabled(&path_str, true)
            .await
            .unwrap_or_else(|e| panic!("缺少 init 表不应报错: {e}"));

        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取配置失败: {e}"));
        let value: toml::Value =
            toml::from_str(&content).unwrap_or_else(|e| panic!("解析配置失败: {e}"));
        assert!(value.get("init").is_none(), "不应凭空新增 init 表");
        assert_eq!(
            value
                .get("server")
                .and_then(|s| s.get("port"))
                .and_then(toml::Value::as_integer),
            Some(8080)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
