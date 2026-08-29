//! 证书管理错误类型（消息 key 由前端 i18n 渲染）。

use ipma_common::AppMessage;

#[derive(Debug, thiserror::Error)]
pub enum CertManagerError {
    #[error("验证失败: {0}")]
    Validation(AppMessage),

    #[error("冲突: {0}")]
    Conflict(AppMessage),

    #[error("未找到: {0}")]
    NotFound(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
}

impl From<CertManagerError> for ipma_common::AppError {
    fn from(err: CertManagerError) -> Self {
        match err {
            CertManagerError::Validation(m) => ipma_common::AppError::Validation(m),
            CertManagerError::Conflict(m) => ipma_common::AppError::Conflict(m),
            CertManagerError::NotFound(m) => ipma_common::AppError::NotFound(m),
            CertManagerError::Internal(m) => ipma_common::AppError::Internal(m),
        }
    }
}
