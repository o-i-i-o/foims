//! 业务通知外发门面：邮件 + Webhook 双渠道一次调用。
//!
//! 站内通知不进本门面：其逐收件人写行与就地刷新语义（告警 episode 按住
//! 通知 ID 改写同一行文案）由各生产方经 [`crate::in_site::create_notification`]
//! 自行实现；本门面只收敛「同一文案按订阅过滤外发两个订阅型渠道」的
//! 成对调用（原先在四个触发点各写一遍）。

use sqlx::PgPool;

use foims_common::AppError;

use crate::kind::NotifyKind;
use crate::lang::LocalizedText;

/// 按 `kind` 向两个订阅型渠道外发同一组中英双语文案：
/// - 邮件按收件人订阅过滤后按收件人语言分组发送，无人订阅/未配置时静默
///   跳过，发送结果返回给调用方按业务语境记日志（单发语义，结果对调用方
///   可见）；
/// - Webhook 按各目标订阅过滤逐条尽力推送，文案按各目标设置的语言取值，
///   单目标失败仅内部留痕（多目标扇出语义，部分失败不算整体失败）。
pub async fn push(
    pool: &PgPool,
    kind: NotifyKind,
    title: &LocalizedText,
    body: &LocalizedText,
) -> Result<(), AppError> {
    crate::smtp::send_notification_email(pool, kind, title, body).await?;
    crate::webhook::send_notification_webhook(pool, kind, title, body).await;
    Ok(())
}
