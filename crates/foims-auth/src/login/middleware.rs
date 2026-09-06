//! 登录/认证相关 HTTP 中间件。
//!
//! 由 login.rs 拆分而来（纯移动）：鉴权中间件与仅限本机端点守卫。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use jsonwebtoken::errors::ErrorKind;
use uuid::Uuid;

use crate::jwt::{JwtUtils, extract_token_from_parts, get_client_info_from_parts};
use crate::provider::AuthProvider;
use foims_common::msg;

/// 吊销检查所需的数据库不可用时返回 503，中间件内无法用 `?` 传播，统一走此响应
fn db_unavailable_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(foims_common::ApiResponse::<()>::error(msg(
            "server.db.operation_failed",
        ))),
    )
        .into_response()
}

pub async fn auth_middleware<P: AuthProvider>(
    State(state): State<Arc<P>>,
    req: Request,
    next: Next,
) -> Response {
    let (mut parts, body) = req.into_parts();

    let Some(token) = extract_token_from_parts(&parts) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.auth_failed",
            ))),
        )
            .into_response();
    };

    let claims = match state.jwt_utils().validate_token(&token) {
        Ok(claims) => claims,
        Err(err) => {
            let error_key = match err.kind() {
                ErrorKind::ExpiredSignature => "server.auth.token_expired",
                _ => "server.auth.invalid_token",
            };
            return (
                StatusCode::UNAUTHORIZED,
                Json(foims_common::ApiResponse::<()>::error(msg(error_key))),
            )
                .into_response();
        }
    };

    if claims.token_type != "access" {
        return (
            StatusCode::UNAUTHORIZED,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.invalid_token",
            ))),
        )
            .into_response();
    }

    // 检查令牌是否已被撤销。数据库不可用时 fail-closed 拒绝请求
    // （与 refresh 流程策略一致），防止 DB 故障期间被吊销令牌继续通行
    match state.pool() {
        Ok(pool) => match crate::jwt::is_token_revoked(&pool.get_conn(), &token).await {
            Ok(true) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(foims_common::ApiResponse::<()>::error(msg(
                        "server.auth.token_revoked",
                    ))),
                )
                    .into_response();
            }
            Ok(false) => {
                // 用户状态与令牌吊销点检查（同一中间件内完成，不延迟到 refresh）：
                //   1. 账户被禁用 → 直接拒绝；
                //   2. 令牌签发时间早于 tokens_invalidated_at（密码重置/权限变更
                //      吊销点）→ 已被吊销，旧 access 令牌立即失效
                let user_id = match Uuid::parse_str(&claims.sub) {
                    Ok(id) => id,
                    Err(_) => {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(foims_common::ApiResponse::<()>::error(msg(
                                "server.auth.auth_failed",
                            ))),
                        )
                            .into_response();
                    }
                };
                match sqlx::query_as::<_, (bool, chrono::DateTime<Utc>)>(
                    "SELECT status, tokens_invalidated_at FROM users WHERE id = $1",
                )
                .bind(user_id)
                .fetch_optional(&pool.get_conn())
                .await
                {
                    Ok(Some((status, invalidated_at))) => {
                        if !status {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(foims_common::ApiResponse::<()>::error(msg(
                                    "server.auth.account_disabled",
                                ))),
                            )
                                .into_response();
                        }
                        if (claims.iat as i64) <= invalidated_at.timestamp() {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(foims_common::ApiResponse::<()>::error(msg(
                                    "server.auth.token_invalidated_relogin",
                                ))),
                            )
                                .into_response();
                        }
                    }
                    // 用户已被删除：令牌随之为失效
                    Ok(None) => {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(foims_common::ApiResponse::<()>::error(msg(
                                "server.auth.auth_failed",
                            ))),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        foims_common::log_error!("log.auth.check_revoke_failed", error = e);
                        return db_unavailable_response();
                    }
                }
            }
            Err(e) => {
                foims_common::log_error!("log.auth.check_revoke_failed", error = e);
                return db_unavailable_response();
            }
        },
        Err(e) => {
            foims_common::log_error!("log.auth.check_revoke_failed", error = e);
            return db_unavailable_response();
        }
    }

    let (ip_address, user_agent) = get_client_info_from_parts(&parts);
    let current_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    if let Some(token_fingerprint) = &claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.device_validation_failed",
            ))),
        )
            .into_response();
    }

    parts.extensions.insert(claims);

    let req = Request::from_parts(parts, body);
    next.run(req).await
}

pub async fn localhost_only_middleware<P: AuthProvider>(req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();

    // 来源判定（回环对端/UDS 才采信 X-Real-IP，fail-close）的真身定义在
    // foims-common::net，与真实 IP 提取共享同一可信代理模型
    if !foims_common::net::is_localhost_request_from_parts(&parts) {
        return (
            StatusCode::FORBIDDEN,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.access_denied",
            ))),
        )
            .into_response();
    }

    let req = Request::from_parts(parts, body);
    next.run(req).await
}
