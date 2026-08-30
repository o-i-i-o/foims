//! 证书管理错误类型（消息 key 由前端 i18n 渲染）。

use foims_common::AppMessage;

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

impl From<CertManagerError> for foims_common::AppError {
    fn from(err: CertManagerError) -> Self {
        match err {
            CertManagerError::Validation(m) => foims_common::AppError::Validation(m),
            CertManagerError::Conflict(m) => foims_common::AppError::Conflict(m),
            CertManagerError::NotFound(m) => foims_common::AppError::NotFound(m),
            CertManagerError::Internal(m) => foims_common::AppError::Internal(m),
        }
    }
}
