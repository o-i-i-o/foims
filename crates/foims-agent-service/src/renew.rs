//! 客户端证书续期处理（POST /agent/v1/renew，设计 docs/agent-design.md §3.4）。
//!
//! 鉴权完全依赖 mTLS：能建立连接即持有站点 CA 签发的 client 证书，续期
//! 不授予任何新权限，仅换发同身份新证书。除链路证书外，请求体携带的
//! PEM 必须与对端 leaf 一致（绑定链路身份），且剩余寿命低于阈值才签发，
//! 防止正常周期请求反复触发重签。签发后服务端同步替换磁盘上的
//! CLIENT_CERT/CLIENT_KEY，后续下载组包携带新证书；QUIC 监听端只锚定
//! CA，替换 client 物料无需重启。错误以 `{"error": "..."}` 简单格式返回
//! （与 ingest 同口径）。

use axum::Json;
use axum::http::StatusCode;
use foims_common::report::{CertRenewRequest, CertRenewResponse};
use foims_common::x509::cert_remaining;
use serde_json::{Value, json};

use crate::cert;
use crate::report_server::ReportContext;

/// 续期门槛：请求证书剩余寿命低于该天数才允许换发（约 90 天窗口）
pub const RENEW_REMAINING_DAYS_THRESHOLD: i64 = 90;

/// 续期单条请求的处理结果（状态码 + JSON 响应体）。
type RenewResult = (StatusCode, Json<Value>);

/// 简单错误响应（与 ingest::error_json 同构）
fn error_json(status: StatusCode, message: String) -> RenewResult {
    (status, Json(json!({ "error": message })))
}

/// 解析请求 PEM 中第一张证书的 DER 字节（与对端 leaf 比对用）。
fn pem_cert_der(cert_pem: &str) -> Result<Vec<u8>, String> {
    let parsed = pem::parse(cert_pem.trim()).map_err(|e| format!("解析证书 PEM 失败: {e}"))?;
    if parsed.tag() != "CERTIFICATE" {
        return Err("PEM 不含证书段".to_string());
    }
    Ok(parsed.contents().to_vec())
}

/// 处理一次续期请求：校验请求体证书与 mTLS 连接证书一致、剩余寿命低于
/// 阈值后重签 client 证书并替换磁盘物料，返回新证书/私钥 PEM。
pub async fn handle_renew(
    _ctx: &ReportContext,
    body: &[u8],
    peer_leaf_der: Option<&[u8]>,
) -> RenewResult {
    let request: CertRenewRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, format!("请求体解析失败: {e}")),
    };

    // 请求体证书 DER 必须与 mTLS 对端 leaf 完全一致（绑定链路身份，
    // 防止用链路证书鉴权却为任意 PEM 换发）
    let Some(peer_der) = peer_leaf_der else {
        return error_json(StatusCode::FORBIDDEN, "连接未携带客户端证书".to_string());
    };
    let body_der = match pem_cert_der(&request.client_cert_pem) {
        Ok(der) => der,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, e),
    };
    if body_der != peer_der {
        foims_common::log_warn!("log.agent.renew_cert_mismatch");
        return error_json(
            StatusCode::FORBIDDEN,
            "请求体证书与连接证书不一致".to_string(),
        );
    }

    // 剩余寿命校验：低于阈值才换发
    let (remaining_days, _) = match cert_remaining(&request.client_cert_pem) {
        Ok(lifetime) => lifetime,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, e),
    };
    if remaining_days >= RENEW_REMAINING_DAYS_THRESHOLD {
        return error_json(
            StatusCode::BAD_REQUEST,
            format!(
                "证书剩余 {remaining_days} 天，未到续期窗口（低于 {RENEW_REMAINING_DAYS_THRESHOLD} 天才可续期）"
            ),
        );
    }

    // 重签并替换磁盘物料（站点 CA 私钥缺失/签发失败返回 500）
    match cert::renew_client_cert().await {
        Ok((cert_pem, key_pem, not_after)) => (
            StatusCode::OK,
            Json(
                serde_json::to_value(CertRenewResponse {
                    cert_pem,
                    key_pem,
                    not_after,
                })
                .unwrap_or_else(|_| json!({"error": "响应序列化失败"})),
            ),
        ),
        Err(e) => {
            foims_common::log_warn!("log.agent.renew_failed", error = e);
            error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("续期签发失败: {e}"),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造占位对端 leaf DER（仅用于证书一致性分支的比对测试）
    fn dummy_der(byte: u8) -> Vec<u8> {
        vec![byte; 32]
    }

    #[test]
    fn pem_cert_der_仅接受证书段() {
        // 非证书段 PEM 报错
        let key_pem = "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n";
        assert!(pem_cert_der(key_pem).is_err());
        // 非 PEM 输入报错
        assert!(pem_cert_der("garbage").is_err());
    }

    #[tokio::test]
    async fn 请求体与连接证书不一致_应403() {
        let ctx = test_context();
        // 结构合法但 DER 与对端占位证书不同的 PEM（AQIDBA== → [1,2,3,4]）
        let request = CertRenewRequest {
            client_cert_pem: "-----BEGIN CERTIFICATE-----\nAQIDBA==\n-----END CERTIFICATE-----\n"
                .to_string(),
        };
        let body = serde_json::to_vec(&request).unwrap_or_default();
        let (status, _) = handle_renew(&ctx, &body, Some(&dummy_der(1))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "证书不一致应拒绝续期");
    }

    #[tokio::test]
    async fn 连接未携带证书_应403() {
        let ctx = test_context();
        let request = CertRenewRequest {
            client_cert_pem: "-----BEGIN CERTIFICATE-----\nAQIDBA==\n-----END CERTIFICATE-----\n"
                .to_string(),
        };
        let body = serde_json::to_vec(&request).unwrap_or_default();
        let (status, _) = handle_renew(&ctx, &body, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn 非法请求体_应400() {
        let ctx = test_context();
        let (status, _) = handle_renew(&ctx, b"not json", Some(&dummy_der(1))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// 空上下文（renew 处理不读池/配置，仅占位满足签名）
    fn test_context() -> ReportContext {
        ReportContext {
            pool: sqlx::PgPool::connect_lazy("postgres://invalid:invalid@127.0.0.1:1/none")
                .unwrap_or_else(|e| panic!("构造懒连接池失败: {e}")),
            report_interval_secs: 60,
            max_report_bytes: 262_144,
            server_version: "0.0.0".to_string(),
        }
    }
}
