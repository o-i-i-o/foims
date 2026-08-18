//! 证书管理错误类型（消息 key 由前端 i18n 渲染）。

use ipma_common::AppMessage;

#[derive(Debug, thiserror::Error)]
pub enum CertManagerError {
    #[error("验证失败: {0}")]
    Validation(AppMessage),

    #[error("未找到: {0}")]
    NotFound(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
}
