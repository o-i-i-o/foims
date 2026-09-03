//! 服务管理错误定义。
//!
//! 所有变体携带 [`AppMessage`]（i18n key + 动态参数），由前端负责翻译。

use foims_common::{AppMessage, msg};
use thiserror::Error;

/// 服务管理模块错误类型。
#[derive(Error, Debug)]
pub enum ServicesError {
    /// 目标服务未注册：标准 systemd 目录中不存在单元文件。
    /// 按约定不做任何回退（直接拉起二进制 / pkill 等），直接向调用方报错。
    #[error("未注册服务: {0}")]
    NotRegistered(AppMessage),

    /// 系统缺少 systemctl（非 systemd 环境），无法执行服务管理。
    #[error("systemctl 不可用: {0}")]
    NotInstalled(AppMessage),

    /// systemctl 操作失败（非零退出码），携带 stderr 摘要。
    #[error("服务操作失败: {0}")]
    Operation(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
}

impl From<ServicesError> for foims_common::AppError {
    fn from(err: ServicesError) -> Self {
        match err {
            ServicesError::NotRegistered(m) => foims_common::AppError::NotFound(m),
            ServicesError::NotInstalled(m)
            | ServicesError::Operation(m)
            | ServicesError::Internal(m) => foims_common::AppError::Internal(m),
        }
    }
}

/// 构造「未注册服务」错误（unit 为完整单元名，如 nginx.service）。
pub fn not_registered(unit: &str) -> ServicesError {
    ServicesError::NotRegistered(msg("server.services.not_registered").with("unit", unit))
}

/// 构造「systemctl 不可用」错误。
pub fn systemctl_missing() -> ServicesError {
    ServicesError::NotInstalled(msg("server.services.systemctl_missing"))
}

/// 构造「服务操作失败」错误（detail 为 stderr 摘要）。
pub fn op_failed(unit: &str, op: &str, detail: &str) -> ServicesError {
    ServicesError::Operation(
        msg("server.services.op_failed")
            .with("unit", unit)
            .with("op", op)
            .with("detail", detail),
    )
}

pub type ServicesResult<T> = Result<T, ServicesError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 未注册错误应映射为_apperror_notfound() {
        let err = not_registered("nginx.service");
        let app: foims_common::AppError = err.into();
        assert!(matches!(app, foims_common::AppError::NotFound(_)));
    }

    #[test]
    fn 操作失败错误应携带单元与操作参数() {
        let err = op_failed("foims.service", "restart", "exit status 1");
        let ServicesError::Operation(m) = err else {
            panic!("应构造 Operation 变体");
        };
        let params = m.params_map().unwrap_or_default();
        assert_eq!(
            params.get("unit").map(String::as_str),
            Some("foims.service")
        );
        assert_eq!(params.get("op").map(String::as_str), Some("restart"));
        assert_eq!(
            params.get("detail").map(String::as_str),
            Some("exit status 1")
        );
    }
}
