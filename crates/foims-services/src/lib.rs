//! FOIMS 服务管理：统一管理 foims 与 nginx 两个 systemd 服务。
//!
//! 管理操作（[`start`](ManagedService::start) / [`stop`](ManagedService::stop) /
//! [`restart`](ManagedService::restart) / [`reload`](ManagedService::reload) /
//! [`enable`](ManagedService::enable) / [`disable`](ManagedService::disable) /
//! [`status`](ManagedService::status)，或经 [`execute`](ManagedService::execute)
//! 以 [`ServiceOp`] 分发）一律先校验单元文件已在标准 systemd 目录注册
//! （[`SYSTEMD_UNIT_DIRS`]）；未注册直接返回
//! [`ServicesError::NotRegistered`]（「未注册服务」），不做任何
//! 「直接拉起二进制 / pkill / 降级轮询」等不当回退。
//!
//! 服务注册不由本程序完成：foims 单元文件随 DEB 包安装
//! （test/build-deb.sh）或由运维手动部署，nginx 单元文件由发行版包提供。

use std::path::PathBuf;

use serde::Serialize;

pub mod error;
pub mod systemd;

pub use error::{ServicesError, ServicesResult};
pub use systemd::{SYSTEMD_UNIT_DIRS, find_unit_file, require_unit_file};

/// 受管服务集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagedService {
    /// FOIMS 后端（DEB 包安装到 /usr/lib/systemd/system，
    /// 或运维手动部署到 /etc/systemd/system）
    Foims,
    /// nginx 反向代理（单元文件由发行版包提供）
    Nginx,
}

impl ManagedService {
    /// 全部受管服务。
    pub const ALL: [ManagedService; 2] = [ManagedService::Foims, ManagedService::Nginx];

    /// 服务标识（API 路径与前端展示用，如 "foims" / "nginx"）。
    pub fn name(self) -> &'static str {
        match self {
            ManagedService::Foims => "foims",
            ManagedService::Nginx => "nginx",
        }
    }

    /// 按服务标识解析（API 路径参数校验用）。
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "foims" => Some(ManagedService::Foims),
            "nginx" => Some(ManagedService::Nginx),
            _ => None,
        }
    }

    /// systemd 单元名。
    pub fn unit_name(self) -> &'static str {
        match self {
            ManagedService::Foims => "foims.service",
            ManagedService::Nginx => "nginx.service",
        }
    }

    /// 是否支持 reload（单元需声明 ExecReload；nginx 支持，foims 不支持）。
    pub fn reload_supported(self) -> bool {
        matches!(self, ManagedService::Nginx)
    }

    /// 定位已注册的单元文件；未注册返回 `None`（探测语义，不报错）。
    pub async fn unit_file(self) -> Option<PathBuf> {
        find_unit_file(self.unit_name()).await
    }

    /// 定位单元文件；未注册直接报「未注册服务」错误（无回退）。
    pub async fn require_unit_file(self) -> ServicesResult<PathBuf> {
        systemd::require_unit_file(self.unit_name()).await
    }

    /// 按操作类型分发执行（未注册即报错）。
    pub async fn execute(self, op: ServiceOp) -> ServicesResult<()> {
        match op {
            ServiceOp::Start => self.start().await,
            ServiceOp::Stop => self.stop().await,
            ServiceOp::Restart => self.restart().await,
            ServiceOp::Reload => self.reload().await,
            ServiceOp::Enable => self.enable().await,
            ServiceOp::Disable => self.disable().await,
        }
    }

    /// 查询服务状态（active / enabled / StatusText 等）。
    pub async fn status(self) -> ServicesResult<ServiceStatus> {
        self.require_unit_file().await?;
        let output = systemd::systemctl_show(
            self.unit_name(),
            &["ActiveState", "SubState", "UnitFileState", "StatusText"],
        )
        .await?;
        Ok(ServiceStatus::from_show_output(&output))
    }

    /// 启动服务（未注册即报错，下同）。
    pub async fn start(self) -> ServicesResult<()> {
        self.require_unit_file().await?;
        systemd::systemctl_op("start", self.unit_name()).await
    }

    /// 停止服务。
    pub async fn stop(self) -> ServicesResult<()> {
        self.require_unit_file().await?;
        systemd::systemctl_op("stop", self.unit_name()).await
    }

    /// 重启服务。
    pub async fn restart(self) -> ServicesResult<()> {
        self.require_unit_file().await?;
        systemd::systemctl_op("restart", self.unit_name()).await
    }

    /// 重载服务配置（单元需声明 ExecReload；nginx 支持，foims 不支持）。
    pub async fn reload(self) -> ServicesResult<()> {
        self.require_unit_file().await?;
        systemd::systemctl_op("reload", self.unit_name()).await
    }

    /// 设置开机自启。
    pub async fn enable(self) -> ServicesResult<()> {
        self.require_unit_file().await?;
        systemd::systemctl_op("enable", self.unit_name()).await
    }

    /// 取消开机自启。
    pub async fn disable(self) -> ServicesResult<()> {
        self.require_unit_file().await?;
        systemd::systemctl_op("disable", self.unit_name()).await
    }
}

/// 服务管理操作（API 路径 `{service}/{op}` 中的 op 段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceOp {
    Start,
    Stop,
    Restart,
    Reload,
    Enable,
    Disable,
}

impl ServiceOp {
    /// 全部受支持的操作。
    pub const ALL: [ServiceOp; 6] = [
        ServiceOp::Start,
        ServiceOp::Stop,
        ServiceOp::Restart,
        ServiceOp::Reload,
        ServiceOp::Enable,
        ServiceOp::Disable,
    ];

    /// 操作名（API 路径参数与日志用）。
    pub fn as_str(self) -> &'static str {
        match self {
            ServiceOp::Start => "start",
            ServiceOp::Stop => "stop",
            ServiceOp::Restart => "restart",
            ServiceOp::Reload => "reload",
            ServiceOp::Enable => "enable",
            ServiceOp::Disable => "disable",
        }
    }

    /// 按操作名解析（API 路径参数校验用）。
    pub fn parse(op: &str) -> Option<Self> {
        ServiceOp::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == op)
    }
}

/// systemctl show 查询结果快照。
#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    /// ActiveState：active / inactive / failed / activating …
    pub active_state: String,
    /// SubState：running / dead / failed …
    pub sub_state: String,
    /// UnitFileState：enabled / disabled / static / masked …
    pub unit_file_state: String,
    /// StatusText：单元自定义状态描述（未设置为 None）
    pub status_text: Option<String>,
}

impl ServiceStatus {
    /// 解析 `systemctl show --property=...` 的 key=value 输出。
    fn from_show_output(output: &str) -> Self {
        let mut active_state = String::new();
        let mut sub_state = String::new();
        let mut unit_file_state = String::new();
        let mut status_text = None;

        for line in output.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "ActiveState" => active_state = value.to_string(),
                "SubState" => sub_state = value.to_string(),
                "UnitFileState" => unit_file_state = value.to_string(),
                "StatusText" if !value.is_empty() => status_text = Some(value.to_string()),
                _ => {}
            }
        }

        Self {
            active_state,
            sub_state,
            unit_file_state,
            status_text,
        }
    }

    /// 服务是否处于 active 状态。
    pub fn is_active(&self) -> bool {
        self.active_state == "active"
    }

    /// 服务是否已设置开机自启（含 enabled-runtime）。
    pub fn is_enabled(&self) -> bool {
        self.unit_file_state.starts_with("enabled")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 单元名映射应正确() {
        assert_eq!(ManagedService::Foims.unit_name(), "foims.service");
        assert_eq!(ManagedService::Nginx.unit_name(), "nginx.service");
        assert_eq!(ManagedService::ALL.len(), 2);
    }

    #[test]
    fn 服务标识与解析应互逆() {
        for svc in ManagedService::ALL {
            assert_eq!(ManagedService::parse(svc.name()), Some(svc));
        }
        assert_eq!(ManagedService::parse("httpd"), None);
        assert_eq!(ManagedService::parse(""), None);
    }

    #[test]
    fn 操作名与解析应互逆() {
        for op in ServiceOp::ALL {
            assert_eq!(ServiceOp::parse(op.as_str()), Some(op));
        }
        assert_eq!(ServiceOp::parse("status"), None);
        assert_eq!(ServiceOp::parse("restart --force"), None);
    }

    #[test]
    fn 仅_nginx_支持_reload() {
        assert!(!ManagedService::Foims.reload_supported());
        assert!(ManagedService::Nginx.reload_supported());
    }

    #[test]
    fn show输出解析应提取各属性() {
        let output = "ActiveState=active\nSubState=running\n\
                      UnitFileState=enabled\nStatusText=FOIMS 运行中\n";
        let status = ServiceStatus::from_show_output(output);
        assert!(status.is_active());
        assert!(status.is_enabled());
        assert_eq!(status.sub_state, "running");
        assert_eq!(status.status_text.as_deref(), Some("FOIMS 运行中"));
    }

    #[test]
    fn 空status_text应视为未设置() {
        let output = "ActiveState=inactive\nSubState=dead\nUnitFileState=disabled\nStatusText=\n";
        let status = ServiceStatus::from_show_output(output);
        assert!(!status.is_active());
        assert!(!status.is_enabled());
        assert!(status.status_text.is_none());
    }
}
