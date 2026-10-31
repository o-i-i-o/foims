//! IPMI 连通性探测 handler（test-ipmi）。
//!
//! 探测入参允许与设备库配置混用（请求字段优先，凭据经解密）；目标地址受
//! SSRF 约束，组播与链路本地段拒绝，回环与私有段只告警放行（与 SNMP 同口径）。
//! 会话端口固定 623（ruipmi 约束），不可由请求覆盖。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::{AppError, log_info, log_warn, msg};
use foims_models::IpmiTestRequest;

use crate::core::{DeviceForIpmi, IPMI_AUTH_SHA256, IpmiParams, get_device_ipmi_config, test_ipmi};

/// POST /api/resources/devices/{id}/test-ipmi：按设备库已存配置发起连通性
/// 探测；路径 `id` 覆盖请求体的 `device_id`，其余口径同
/// `/api/resources/devices/test-ipmi`。
pub async fn test_ipmi_connection_by_id<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
    AppJson(req): AppJson<IpmiTestRequest>,
) -> Result<Response, AppError> {
    let mut test_req = req;
    test_req.device_id = Some(device_id);
    test_ipmi_connection(State(state), AppJson(test_req)).await
}

/// POST /api/resources/devices/test-ipmi：向目标地址发起一次性 IPMI RMCP+ 探测，
/// 成功返回 Get Device ID 摘要。请求字段优先、缺省回退设备库配置（口令经解密）；
/// 设备不存在 404，IP 缺失/非法/组播/链路本地 400，凭据缺失 400，探测失败 400。
pub async fn test_ipmi_connection<P: DbProvider>(
    State(state): State<Arc<P>>,
    AppJson(req): AppJson<IpmiTestRequest>,
) -> Result<Response, AppError> {
    let device_id_label = req.device_id.map(|d| d.to_string()).unwrap_or_default();
    let ip_label = req.ip_address.clone().unwrap_or_default();
    log_info!(
        "log.device.ipmi.test_request",
        device_id = device_id_label,
        ip = ip_label
    );

    // 解析目标 IP 与凭据：请求字段优先，缺省回退设备库配置（口令解密）
    let (ip, username, password, auth_algo) = if let Some(device_id) = req.device_id {
        let conn = state.pool()?.get_conn();
        let (device, ip_address) = get_device_ipmi_config(&conn, &device_id).await?;
        let params = load_params_from_device(&device, ip_address.as_deref()).await?;
        (
            req.ip_address.or_else(|| ip_address.clone()),
            req.ipmi_username.or(params.username),
            req.ipmi_password.or(params.password),
            req.ipmi_auth_algo.or(params.auth_algo),
        )
    } else {
        (
            req.ip_address,
            req.ipmi_username,
            req.ipmi_password,
            req.ipmi_auth_algo,
        )
    };

    let ip = match ip {
        Some(s) if !s.is_empty() => s,
        _ => return Err(AppError::Validation(msg("server.device.ipmi.ip_required"))),
    };

    // 拒绝组播/链路本地地址防止 SSRF；回环与私有地址属机房管理正常目标，仅告警放行
    let parsed_ip: std::net::IpAddr = ip
        .parse()
        .map_err(|_| AppError::Validation(msg("server.device.ipmi.ip_invalid").with("ip", &ip)))?;
    if parsed_ip.is_loopback() {
        log_warn!("log.device.ipmi.loopback_ip_allowed", ip = ip.clone());
    }
    if parsed_ip.is_multicast() {
        return Err(AppError::Validation(msg(
            "server.device.ipmi.multicast_forbidden",
        )));
    }
    match parsed_ip {
        std::net::IpAddr::V4(v4) => {
            if v4.is_link_local() {
                return Err(AppError::Validation(msg(
                    "server.device.ipmi.link_local_forbidden",
                )));
            }
            if v4.is_private() {
                log_warn!("log.device.ipmi.private_ip_allowed", ip = ip.clone());
            }
        }
        std::net::IpAddr::V6(v6) => {
            if v6.is_unicast_link_local() {
                return Err(AppError::Validation(msg(
                    "server.device.ipmi.link_local_forbidden",
                )));
            }
            if v6.is_unique_local() {
                log_warn!("log.device.ipmi.ula_ip_allowed", ip = ip.clone());
            }
        }
    }

    let username = username
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::Validation(msg("server.device.ipmi.credentials_required")))?;
    let password = password
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::Validation(msg("server.device.ipmi.credentials_required")))?;

    let params = IpmiParams {
        ip: ip.clone(),
        username,
        password,
        auth_algo: auth_algo
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| IPMI_AUTH_SHA256.to_string()),
    };

    // 凭据属敏感物料，日志只记目标与算法、不落明文
    log_info!(
        "log.device.ipmi.test_params",
        ip = ip,
        auth_algo = params.auth_algo.clone()
    );

    match test_ipmi(&params).await {
        Ok(summary) => Ok(foims_common::ok_json(
            serde_json::json!({ "summary": summary }),
            "server.device.ipmi.test_success",
        )),
        Err(e) => Err(AppError::Ipmi(
            msg("server.device.ipmi.test_failed").with("error", e.to_string()),
        )),
    }
}

/// 从设备行构造可选会话参数（凭据解密）：未配置（用户名或口令缺失）返回全 None，
/// 由 handler 判定 `credentials_required`。
async fn load_params_from_device(
    device: &DeviceForIpmi,
    ip_address: Option<&str>,
) -> Result<IpmiParamsOpt, AppError> {
    let ip = ip_address.unwrap_or_default();
    match device.to_ipmi_params_async(ip).await? {
        Some(p) => Ok(IpmiParamsOpt {
            username: Some(p.username),
            password: Some(p.password),
            auth_algo: Some(p.auth_algo),
        }),
        None => Ok(IpmiParamsOpt {
            username: None,
            password: None,
            auth_algo: device.ipmi_auth_algo.clone().filter(|v| !v.is_empty()),
        }),
    }
}

/// 设备侧解密后的凭据片段（口令为明文，仅在本函数到 IpmiParams 之间流转）。
struct IpmiParamsOpt {
    username: Option<String>,
    password: Option<String>,
    auth_algo: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试桩：pool 为 None，迫使探测走直接 IP 输入路径，不触及数据库分支
    #[derive(Clone)]
    struct TestState {
        pool: Option<foims_common::db::DbPool>,
    }

    impl foims_common::DbProvider for TestState {
        fn pool(&self) -> Result<&foims_common::db::DbPool, foims_common::AppError> {
            self.pool.as_ref().ok_or_else(|| {
                foims_common::AppError::Internal(foims_common::msg("server.db.not_initialized"))
            })
        }
    }

    fn make_state() -> Arc<TestState> {
        Arc::new(TestState { pool: None })
    }

    /// 发起 IPMI 连接测试并断言返回的错误消息 key
    async fn assert_ipmi_ip_rejected(ip: Option<&str>, expected_key: &str) {
        let req = foims_models::IpmiTestRequest {
            device_id: None,
            ip_address: ip.map(str::to_string),
            ipmi_username: Some("admin".to_string()),
            ipmi_password: Some("secret".to_string()),
            ipmi_auth_algo: Some("sha256".to_string()),
        };
        let result = test_ipmi_connection(State(make_state()), AppJson(req)).await;
        let err = result.err().unwrap_or_else(|| panic!("IP {ip:?} 应被拒绝"));
        assert!(
            matches!(err, AppError::Validation(_)),
            "应返回 Validation 错误，实际: {err}"
        );
        assert_eq!(
            err.message().key(),
            expected_key,
            "IP {ip:?} 的拒绝原因不符"
        );
    }

    #[tokio::test]
    async fn test_ipmi_rejects_multicast_addresses() {
        assert_ipmi_ip_rejected(Some("224.0.0.1"), "server.device.ipmi.multicast_forbidden").await;
        assert_ipmi_ip_rejected(Some("ff02::1"), "server.device.ipmi.multicast_forbidden").await;
    }

    #[tokio::test]
    async fn test_ipmi_rejects_link_local_and_metadata_addresses() {
        assert_ipmi_ip_rejected(
            Some("169.254.1.1"),
            "server.device.ipmi.link_local_forbidden",
        )
        .await;
        assert_ipmi_ip_rejected(
            Some("169.254.169.254"),
            "server.device.ipmi.link_local_forbidden",
        )
        .await;
    }

    #[tokio::test]
    async fn test_ipmi_rejects_invalid_or_missing_ip() {
        assert_ipmi_ip_rejected(Some("not-an-ip"), "server.device.ipmi.ip_invalid").await;
        assert_ipmi_ip_rejected(None, "server.device.ipmi.ip_required").await;
        assert_ipmi_ip_rejected(Some(""), "server.device.ipmi.ip_required").await;
    }

    #[tokio::test]
    async fn test_ipmi_requires_credentials() {
        let req = foims_models::IpmiTestRequest {
            device_id: None,
            ip_address: Some("192.0.2.1".to_string()),
            ipmi_username: None,
            ipmi_password: None,
            ipmi_auth_algo: None,
        };
        let result = test_ipmi_connection(State(make_state()), AppJson(req)).await;
        let err = result.err().unwrap_or_else(|| panic!("缺凭据应被拒绝"));
        assert_eq!(
            err.message().key(),
            "server.device.ipmi.credentials_required"
        );
    }
}
