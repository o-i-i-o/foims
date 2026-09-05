//! 初始化功能配置（备份目录、启用开关与数据库连接信息写盘）。

use crate::types::DatabaseConfig;

pub fn get_backup_dir() -> String {
    if let Some(home) = std::env::var_os("HOME") {
        format!("{}/foims_backups", home.to_string_lossy())
    } else {
        "/opt/foims/backups".to_string()
    }
}

/// 原子化写回配置文件：先写同目录临时文件再 rename 覆盖。config.toml
/// 承载 init.enabled 等安全开关，直接覆写时进程中途崩溃会留下截断的
/// 配置文件，导致重启后无法解析。
///
/// 写入失败与 rename 失败同样清理临时文件，避免残留堆积；
/// rename 前 fsync 确保内容落盘，掉电后 rename 出的配置文件不缺页；
/// 临时文件继承原文件权限（rename 替换后保留原有访问控制）。
async fn atomic_write_toml(
    config_path: &str,
    value: &toml::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let new_content = toml::to_string(value)?;
    let tmp_path = format!("{config_path}.{}.tmp", uuid::Uuid::new_v4());
    let write_result: std::io::Result<()> = async {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::File::create(&tmp_path).await?;
        file.write_all(new_content.as_bytes()).await?;
        file.sync_all().await?;
        Ok(())
    }
    .await;
    if let Err(e) = write_result {
        if let Err(remove_err) = tokio::fs::remove_file(&tmp_path).await {
            foims_common::log_warn!("log.init.config_tmp_remove_failed", error = remove_err);
        }
        return Err(e.into());
    }

    if let Ok(meta) = tokio::fs::metadata(config_path).await
        && let Err(e) = tokio::fs::set_permissions(&tmp_path, meta.permissions()).await
    {
        foims_common::log_warn!("log.init.config_tmp_chmod_failed", error = e);
    }

    if let Err(e) = tokio::fs::rename(&tmp_path, config_path).await {
        if let Err(remove_err) = tokio::fs::remove_file(&tmp_path).await {
            foims_common::log_warn!("log.init.config_tmp_remove_failed", error = remove_err);
        }
        return Err(e.into());
    }
    Ok(())
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
    atomic_write_toml(config_path, &value).await
}

/// 将连接测试通过的数据库连接五要素写入配置文件 [database] 表。
///
/// 只更新 host/port/database/username/password；max_connections 等池参数
/// 与文件内其余键原样保留。失败参数不落盘：调用方仅在测试通过后调用。
pub async fn update_config_database(
    config_path: &str,
    db: &DatabaseConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = tokio::fs::read_to_string(config_path).await?;
    let mut value: toml::Value = toml::from_str(&content)?;
    let Some(database) = value.get_mut("database") else {
        return Err(format!("配置文件缺少 [database] 表: {config_path}").into());
    };
    let Some(table) = database.as_table_mut() else {
        return Err(format!("配置文件 [database] 不是表结构: {config_path}").into());
    };
    table.insert("host".to_string(), toml::Value::String(db.host.clone()));
    table.insert("port".to_string(), toml::Value::Integer(i64::from(db.port)));
    table.insert(
        "database".to_string(),
        toml::Value::String(db.database.clone()),
    );
    table.insert(
        "username".to_string(),
        toml::Value::String(db.username.clone()),
    );
    table.insert(
        "password".to_string(),
        toml::Value::String(db.password.clone()),
    );
    atomic_write_toml(config_path, &value).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建进程唯一的临时目录，返回其路径（测试结束后由调用方清理）
    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "foims_init_config_test_{}_{:?}",
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
            let expected = format!("{}/foims_backups", home.to_string_lossy());
            assert_eq!(dir, expected);
        } else {
            assert_eq!(dir, "/opt/foims/backups");
        }
    }

    /// 更新 enabled 开关并保留同表其他键
    #[tokio::test]
    async fn update_config_enabled_修改开关并保留其他键() {
        let dir = temp_dir();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[init]\nenabled = false\nname = \"foims\"\n")
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
        assert_eq!(
            init.get("name").and_then(toml::Value::as_str),
            Some("foims")
        );

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

    /// 构造带池参数的最小 [database] 配置
    fn sample_db_config() -> DatabaseConfig {
        DatabaseConfig {
            host: "192.168.1.10".to_string(),
            port: 5433,
            database: "foims_prod".to_string(),
            username: "foims".to_string(),
            password: "p@ss:w0rd".to_string(),
            max_connections: 10,
            min_connections: 5,
            acquire_timeout_secs: 15,
            idle_timeout_secs: 60,
            max_lifetime_secs: 1800,
            query_timeout_secs: 30,
            health_check_interval_secs: 30,
        }
    }

    /// 写入连接五要素并保留池参数与其余配置键
    #[tokio::test]
    async fn update_config_database_更新连接要素并保留池参数() {
        let dir = temp_dir();
        let path = dir.join("db_config.toml");
        std::fs::write(
            &path,
            "[database]\nhost = \"localhost\"\nport = 5432\ndatabase = \"foims\"\nusername = \"username\"\npassword = \"password\"\nmax_connections = 20\nmin_connections = 5\n\n[init]\nenabled = true\n",
        )
        .unwrap_or_else(|e| panic!("写入测试配置失败: {e}"));

        let path_str = path.to_string_lossy().to_string();
        update_config_database(&path_str, &sample_db_config())
            .await
            .unwrap_or_else(|e| panic!("更新数据库配置失败: {e}"));

        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取配置失败: {e}"));
        let value: toml::Value =
            toml::from_str(&content).unwrap_or_else(|e| panic!("解析配置失败: {e}"));
        let db = value
            .get("database")
            .unwrap_or_else(|| panic!("应保留 database 表"));
        // 连接五要素更新为传入值
        assert_eq!(
            db.get("host").and_then(toml::Value::as_str),
            Some("192.168.1.10")
        );
        assert_eq!(db.get("port").and_then(toml::Value::as_integer), Some(5433));
        assert_eq!(
            db.get("database").and_then(toml::Value::as_str),
            Some("foims_prod")
        );
        assert_eq!(
            db.get("username").and_then(toml::Value::as_str),
            Some("foims")
        );
        assert_eq!(
            db.get("password").and_then(toml::Value::as_str),
            Some("p@ss:w0rd")
        );
        // 池参数保留文件原值，不被默认值覆盖
        assert_eq!(
            db.get("max_connections").and_then(toml::Value::as_integer),
            Some(20)
        );
        // 其余配置键不受影响
        assert_eq!(
            value
                .get("init")
                .and_then(|i| i.get("enabled"))
                .and_then(toml::Value::as_bool),
            Some(true)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 缺少 [database] 表时报错而非凭空新增
    #[tokio::test]
    async fn update_config_database_缺少database表时报错() {
        let dir = temp_dir();
        let path = dir.join("no_db.toml");
        std::fs::write(&path, "[init]\nenabled = true\n")
            .unwrap_or_else(|e| panic!("写入测试配置失败: {e}"));

        let path_str = path.to_string_lossy().to_string();
        let result = update_config_database(&path_str, &sample_db_config()).await;
        assert!(result.is_err(), "缺少 [database] 表应报错");

        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取配置失败: {e}"));
        let value: toml::Value =
            toml::from_str(&content).unwrap_or_else(|e| panic!("解析配置失败: {e}"));
        assert!(value.get("database").is_none(), "不应凭空新增 database 表");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
