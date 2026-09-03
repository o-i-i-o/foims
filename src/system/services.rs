//! 服务管理接口（系统配置 - 服务管理卡片）。
//!
//! 统一管理 foims 与 nginx 两个 systemd 服务：
//! - 状态查询：`GET /api/system/services`
//! - 管理操作：`POST /api/system/services/{service}/{op}`
//!   （service ∈ {foims, nginx}，op ∈ {start, stop, restart, reload, enable, disable}）
//!
//! 服务注册不由本程序完成（DEB 包安装或运维手动部署单元文件）；
//! 未注册的服务操作一律返回「未注册服务」错误，不做回退。
//! foims 重启为自重启特例：先返回响应，由后台延迟执行，避免重启本进程
//! 导致响应不可达；独立进程模式下退化为脚本重启。

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Path;
use axum::response::Response;
use serde::Serialize;
use uuid::Uuid;

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
    /// 本进程是否以 systemd 服务方式运行（仅 foims 有意义）
    pub running_as_service: Option<bool>,
    /// 服务运行时长（仅 foims：以本进程启动时间计算）
    pub uptime_seconds: Option<u64>,
}

/// 查询全部受管服务状态（探测语义：单服务查询失败降级为默认字段并告警）。
pub async fn get_services_status(
    _admin: foims_auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let running_as_service = tokio::task::spawn_blocking(check_if_running_as_service)
        .await
        .unwrap_or(false);

    let mut items = Vec::with_capacity(ManagedService::ALL.len());
    for svc in ManagedService::ALL {
        items.push(build_status_item(svc, running_as_service).await);
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
async fn build_status_item(svc: ManagedService, running_as_service: bool) -> ServiceStatusItem {
    // 注册状态经 foims-services 在全部标准 systemd 目录中探测
    // （DEB 包装到 /usr/lib/systemd/system，手动部署写入 /etc/systemd/system）
    let registered = svc.unit_file().await.is_some();

    // 状态查询为探测语义：查询失败记录告警并返回默认字段，
    // 由 registered 字段如实反映注册状态
    let status = if registered {
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

    let (active, active_state, sub_state, enabled, unit_file_state, status_text) = match status {
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
    let uptime_seconds = if is_foims && active {
        Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(start_time()),
        )
    } else {
        None
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
        running_as_service: is_foims.then_some(running_as_service),
        uptime_seconds,
    }
}

/// 重启 foims 服务（或独立进程模式下的程序自身）。
///
/// 服务模式下由后台延迟执行 `systemctl restart foims`；独立进程模式下
/// 经分离脚本重新拉起二进制。响应先行返回，避免重启导致响应不可达。
pub async fn trigger_service_restart() -> Result<Response, AppError> {
    let is_running_as_service = tokio::task::spawn_blocking(check_if_running_as_service)
        .await
        .unwrap_or(false);

    log_info!(
        "log.system.restart_triggered",
        as_service = is_running_as_service
    );

    if is_running_as_service {
        // 预检服务注册状态：未注册直接向调用方报「未注册服务」，
        // 不做独立进程重启等回退
        ManagedService::Foims.require_unit_file().await?;

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
    } else {
        log_info!("log.system.standalone_restart");
        restart_standalone_process().await
    }
}

/// 是否以 systemd 服务方式运行。
///
/// 仅认 systemd 自身的运行痕迹（INVOCATION_ID / cgroup 归属）；
/// 「单元文件存在」不代表本进程由 systemd 拉起，不作为判据。
fn check_if_running_as_service() -> bool {
    if std::env::var("INVOCATION_ID").is_ok() {
        return true;
    }

    if let Ok(cgroup) = std::fs::read_to_string("/proc/self/cgroup")
        && (cgroup.contains("systemd") || cgroup.contains(".service"))
    {
        return true;
    }

    false
}

/// 独立进程模式的重启：分离 shell 延迟重新拉起本二进制后退出。
async fn restart_standalone_process() -> Result<Response, AppError> {
    let exe_path = std::env::current_exe()
        .map_err(|e| AppError::Internal(msg("server.system.exe_path_failed").with("error", e)))?;

    let exe_path_str = exe_path
        .to_str()
        .ok_or_else(|| AppError::Internal(msg("server.system.exe_path_invalid")))?;

    let working_dir = std::env::current_dir()
        .map_err(|e| AppError::Internal(msg("server.system.workdir_failed").with("error", e)))?;

    let working_dir_str = working_dir
        .to_str()
        .ok_or_else(|| AppError::Internal(msg("server.system.workdir_invalid")))?;

    let restart_script = r#"#!/bin/bash
sleep 3
cd "$1"
exec "$2"
"#;

    // 随机文件名 + create_new 原子创建（0700）：避免固定路径被本地低权用户
    // 预置符号链接劫持为任意文件写入/执行（security-review I-5）
    let script_path = format!("/tmp/foims_restart_{}.sh", Uuid::new_v4());
    {
        // tokio::fs::OpenOptions 在 Unix 上原生提供 mode()
        let mut opts = tokio::fs::OpenOptions::new();
        opts.mode(0o700).write(true).create_new(true);
        let mut file = opts.open(&script_path).await.map_err(|e| {
            AppError::Internal(msg("server.system.restart_script_create_failed").with("error", e))
        })?;
        use tokio::io::AsyncWriteExt;
        file.write_all(restart_script.as_bytes())
            .await
            .map_err(|e| {
                AppError::Internal(
                    msg("server.system.restart_script_create_failed").with("error", e),
                )
            })?;
    }

    let script_path_owned = script_path.clone();
    // 脚本已 0700 可执行，直接 spawn；退出前清理。
    // spawn 失败意味着重启流程无法继续：保持服务运行并返回错误，
    // 不再无条件 exit(0)（否则 standalone 模式服务直接下线）
    if let Err(e) = tokio::process::Command::new("nohup")
        .arg(&script_path)
        .arg(working_dir_str)
        .arg(exe_path_str)
        .spawn()
    {
        log_warn!("log.system.restart_script_spawn_failed", error = e);
        return Err(AppError::Internal(
            msg("server.system.restart_script_spawn_failed").with("error", e),
        ));
    }

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = tokio::fs::remove_file(&script_path_owned).await;
        std::process::exit(0);
    });

    Ok(foims_common::ok_json(
        (),
        "server.system.restart_command_sent",
    ))
}
