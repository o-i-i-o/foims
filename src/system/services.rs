//! 服务管理接口（系统配置 - 服务管理卡片）。
//!
//! 统一管理 foims 与 nginx 两个 systemd 服务：
//! - 状态查询：`GET /api/system/services`
//! - 管理操作：`POST /api/system/services/{service}/{op}`
//!   （service ∈ {foims, nginx}，op ∈ {start, stop, restart, reload, enable, disable}）
//!
//! 服务注册不由本程序完成（DEB 包安装或运维手动部署单元文件）；
//! 未注册的服务操作一律返回「未注册服务」错误，不做回退。
//! foims 重启为自重启特例：先返回响应，由后台延迟执行 systemctl restart，
//! 避免重启本进程导致响应不可达。

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Path;
use axum::response::Response;
use serde::Serialize;

use crate::system::config::start_time;
use foims_common::{AppError, log_error, log_info, log_warn, msg};
use foims_services::{ManagedService, ServiceOp};

/// 单个受管服务的状态视图。
#[derive(Debug, Serialize)]
pub struct ServiceStatusItem {
    /// 服务标识（foims / nginx）
    pub name: &'static str,
    /// systemd 单元名
    pub unit: &'static str,
    /// 单元文件是否已在标准 systemd 目录注册
    pub registered: bool,
    /// 是否处于 active 状态
    pub active: bool,
    /// ActiveState 原值（active / inactive / failed …）
    pub active_state: String,
    /// SubState 原值（running / dead …）
    pub sub_state: String,
    /// 是否已设置开机自启
    pub enabled: bool,
    /// UnitFileState 原值（enabled / disabled / static …）
    pub unit_file_state: String,
    /// 单元自定义状态描述（未设置为 None）
    pub status_text: Option<String>,
    /// 是否支持 reload（单元需声明 ExecReload）
    pub reload_supported: bool,
    /// 服务运行时长（秒）：foims 按本进程启动时间、nginx 按 systemd
    /// 上次进入 active 的时刻计算
    pub uptime_seconds: Option<u64>,
}

/// 查询全部受管服务状态（探测语义：单服务查询失败降级为默认字段并告警）。
pub async fn get_services_status(
    _admin: foims_auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let mut items = Vec::with_capacity(ManagedService::ALL.len());
    for svc in ManagedService::ALL {
        items.push(build_status_item(svc).await);
    }

    Ok(foims_common::ok_json(
        items,
        "server.services.status_retrieved",
    ))
}

/// 执行服务管理操作（service / op 均来自路径参数，严格白名单校验）。
pub async fn service_operation(
    _admin: foims_auth::extractor::AdminUser,
    Path((service, op)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let svc = ManagedService::parse(&service).ok_or_else(|| {
        AppError::Validation(msg("server.services.unknown_service").with("service", service))
    })?;
    let op = ServiceOp::parse(&op)
        .ok_or_else(|| AppError::Validation(msg("server.services.unknown_op").with("op", op)))?;

    // foims 重启走自重启流程：成功重启会杀死本进程，若直接 await 响应不可达，
    // 由 trigger_service_restart 先返回响应再后台延迟执行
    if svc == ManagedService::Foims && op == ServiceOp::Restart {
        return trigger_service_restart().await;
    }

    log_info!(
        "log.services.op_executed",
        unit = svc.unit_name(),
        op = op.as_str()
    );
    svc.execute(op).await?;
    Ok(foims_common::ok_json(
        (),
        msg("server.services.op_success")
            .with("unit", svc.unit_name())
            .with("op", op.as_str()),
    ))
}

/// 组装单个服务的状态视图；未注册时除 registered 外均为默认值。
async fn build_status_item(svc: ManagedService) -> ServiceStatusItem {
    // 注册状态经 foims-services 在全部标准 systemd 目录中探测
    // （DEB 包装到 /usr/lib/systemd/system，手动部署写入 /etc/systemd/system）
    let registered = svc.unit_file().await.is_some();

    // 状态查询为探测语义：查询失败记录告警并返回默认字段，
    // 由 registered 字段如实反映注册状态
    let systemd_status = if registered {
        match svc.status().await {
            Ok(st) => Some(st),
            Err(e) => {
                log_warn!("log.system.service_status_query_failed", error = e);
                None
            }
        }
    } else {
        None
    };

    // 非 foims 服务的运行时长：按 systemd 上次进入 active 的时刻计算
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let systemd_uptime = systemd_status
        .as_ref()
        .and_then(|st| st.uptime_seconds_since(now_secs));

    let (active, active_state, sub_state, enabled, unit_file_state, status_text) =
        match systemd_status {
            Some(st) => {
                // 先取借用值再逐字段移动，避免部分移动后继续借用
                let active = st.is_active();
                let enabled = st.is_enabled();
                (
                    active,
                    st.active_state,
                    st.sub_state,
                    enabled,
                    st.unit_file_state,
                    st.status_text,
                )
            }
            None => (
                false,
                String::new(),
                String::new(),
                false,
                String::new(),
                None,
            ),
        };

    let is_foims = svc == ManagedService::Foims;
    // 运行时长（秒）：
    // - foims：systemd 单元 active 时按本进程启动时间计算——单元未运行说明
    //   本进程并非由该单元托管，此时不展示运行时长；
    // - 其他服务（nginx）：采用上方 systemd 计算的时长
    let uptime_seconds = if is_foims {
        (active && start_time() > 0).then(|| now_secs.saturating_sub(start_time()))
    } else {
        systemd_uptime
    };

    ServiceStatusItem {
        name: svc.name(),
        unit: svc.unit_name(),
        registered,
        active,
        active_state,
        sub_state,
        enabled,
        unit_file_state,
        status_text,
        reload_supported: svc.reload_supported(),
        uptime_seconds,
    }
}

/// 重启 foims 服务（systemctl restart，后台延迟执行的自重启特例）。
///
/// 先校验单元已注册（未注册直接报「未注册服务」，无回退），再由后台
/// 延迟执行 `systemctl restart foims`。响应先行返回，避免重启导致响应不可达。
pub async fn trigger_service_restart() -> Result<Response, AppError> {
    // 预检服务注册状态：未注册直接向调用方报「未注册服务」，
    // 不做独立进程重启等回退
    ManagedService::Foims.require_unit_file().await?;

    log_info!("log.system.restart_triggered");

    // 在后台延迟执行 systemctl restart：若直接 await，成功重启会杀死本进程导致响应不可达。
    // 先返回响应，由后台任务触发重启。
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        log_info!("log.system.systemctl_restart", service = "foims.service");
        match ManagedService::Foims.restart().await {
            Ok(()) => {
                // 给 systemctl 一点时间终止本进程；若仍存活则主动退出（systemd Restart=always 会拉起）
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                log_info!("log.system.systemctl_exit_fallback");
                std::process::exit(0);
            }
            // 重启命令失败（权限不足等）：保持服务运行并记录错误，
            // 不再盲目 exit(0)（否则健康进程被误杀且无诊断信息）
            Err(e) => {
                log_error!("log.system.systemctl_restart_failed", error = e);
            }
        }
    });

    Ok(foims_common::ok_json(
        (),
        "server.system.restart_command_sent",
    ))
}
