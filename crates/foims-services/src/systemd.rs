//! systemd 单元文件定位与 systemctl 调用封装。
//!
//! 定位规则与 systemd 单元加载优先级一致（/etc 覆盖 /run 覆盖 /usr/lib）；
//! systemctl 调用不做任何回退：命令缺失或非零退出一律返回错误。

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::error::{ServicesError, ServicesResult, not_registered, op_failed, systemctl_missing};

/// systemd 单元文件搜索目录（按加载优先级从高到低）。
///
/// - `/etc/systemd/system`：管理员手工安装（register-service 端点写入处）
/// - `/run/systemd/system`：运行时生成的单元
/// - `/usr/lib/systemd/system`：DEB 包安装位置（test/build-deb.sh）
/// - `/lib/systemd/system`：非合并 usr 系统的发行版单元目录
/// - `/usr/local/lib/systemd/system`：本地手工安装
pub const SYSTEMD_UNIT_DIRS: [&str; 5] = [
    "/etc/systemd/system",
    "/run/systemd/system",
    "/usr/lib/systemd/system",
    "/lib/systemd/system",
    "/usr/local/lib/systemd/system",
];

/// 在指定目录列表中查找单元文件，返回首个命中的路径。
///
/// 目录列表可注入以便测试；探测用 `try_exists`，单个目录探测失败
/// （如权限不足）按未命中处理并继续。
pub async fn find_unit_file_in<I, P>(dirs: I, unit: &str) -> Option<PathBuf>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    for dir in dirs {
        let candidate = dir.as_ref().join(unit);
        if tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
            return Some(candidate);
        }
    }
    None
}

/// 在标准 systemd 目录中查找单元文件。
pub async fn find_unit_file(unit: &str) -> Option<PathBuf> {
    find_unit_file_in(SYSTEMD_UNIT_DIRS, unit).await
}

/// 要求单元已注册并返回其路径；任何标准目录均不存在时直接报
/// 「未注册服务」错误（无回退）。
pub async fn require_unit_file(unit: &str) -> ServicesResult<PathBuf> {
    find_unit_file(unit)
        .await
        .ok_or_else(|| not_registered(unit))
}

/// 执行不携带单元参数的 systemctl 子命令（如 daemon-reload）。
pub async fn systemctl_global(op: &str) -> ServicesResult<()> {
    let output = Command::new("systemctl")
        .arg(op)
        .output()
        .await
        .map_err(|e| classify_spawn_error(e, op, ""))?;
    check_output(op, "", &output)
}

/// 执行携带单元参数的 systemctl 子命令（如 restart nginx.service）。
pub async fn systemctl_op(op: &str, unit: &str) -> ServicesResult<()> {
    let output = Command::new("systemctl")
        .args([op, unit])
        .output()
        .await
        .map_err(|e| classify_spawn_error(e, op, unit))?;
    check_output(op, unit, &output)
}

/// 查询单元属性（systemctl show），返回标准输出原始文本（key=value 行）。
pub async fn systemctl_show(unit: &str, properties: &[&str]) -> ServicesResult<String> {
    let property_arg = properties.join(",");
    let output = Command::new("systemctl")
        .args(["show", unit, "--property", &property_arg])
        .output()
        .await
        .map_err(|e| classify_spawn_error(e, "show", unit))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(op_failed(unit, "show", &detail));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// systemctl 可执行文件缺失归类为 NotInstalled，其余 spawn 失败为内部错误。
fn classify_spawn_error(err: std::io::Error, op: &str, unit: &str) -> ServicesError {
    if err.kind() == ErrorKind::NotFound {
        return systemctl_missing();
    }
    ServicesError::Internal(
        foims_common::msg("server.services.op_failed")
            .with("unit", unit)
            .with("op", op)
            .with("detail", err),
    )
}

fn check_output(op: &str, unit: &str, output: &std::process::Output) -> ServicesResult<()> {
    if output.status.success() {
        return Ok(());
    }
    let detail = if output.stderr.is_empty() {
        format!("exit status {}", output.status.code().unwrap_or_default())
    } else {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    };
    Err(op_failed(unit, op, &detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 构造一次性临时目录（无 tempfile 依赖）。
    fn temp_dir(tag: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("foims-services-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).ok();
        base
    }

    #[tokio::test]
    async fn 未注册单元应返回_none() {
        let base = temp_dir("missing");
        assert!(
            find_unit_file_in([&base], "foims-test-absent.service")
                .await
                .is_none()
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn 注册检测应按目录优先级取首个命中() {
        let base = temp_dir("priority");
        let (etc, usr) = (base.join("etc"), base.join("usr"));
        fs::create_dir_all(&usr).ok();
        fs::write(usr.join("foims-test.service"), b"[Unit]\n").ok();
        // 仅低优先级目录存在：命中 usr
        let found = find_unit_file_in([&etc, &usr], "foims-test.service").await;
        assert_eq!(found, Some(usr.join("foims-test.service")));
        // 高优先级目录存在后：命中 etc
        fs::create_dir_all(&etc).ok();
        fs::write(etc.join("foims-test.service"), b"[Unit]\n").ok();
        let found = find_unit_file_in([&etc, &usr], "foims-test.service").await;
        assert_eq!(found, Some(etc.join("foims-test.service")));
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn 标准目录中不存在的单元应报未注册服务() {
        // 扰动后缀保证任何环境的标准目录中都不存在该单元
        let err = require_unit_file("foims-test-absent-1717.service")
            .await
            .err();
        let Some(err) = err else {
            panic!("未注册单元必须报错，不得回退");
        };
        assert!(matches!(err, ServicesError::NotRegistered(_)));
    }
}
