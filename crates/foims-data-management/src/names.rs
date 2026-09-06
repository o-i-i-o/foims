//! 名称引用的编解码与网络地址工具：导出与导入共用。
//!
//! 复合引用编码约定：
//! - 房间内名称（工位/机柜）：`房间/名称`；
//! - 机位：`房间/机柜/机位名`；
//! - 设备：`房间/设备名`；
//! - 设备端口：`房间/设备名:端口号`；
//! - 配线架端点：`房间/机柜/配线架名`。
//!
//! 解析时按段数切分，末段允许包含分隔符之外的全部内容。

/// 解析 "房间/名称"。
pub fn split_room_scoped(value: &str) -> Option<(&str, &str)> {
    let (room, name) = value.split_once('/')?;
    if !validate_segmented(room, name) {
        return None;
    }
    Some((room, name))
}

/// 解析 "房间/机柜/机位名"（机位/配线架名作为末段）。
pub fn split_cabinet_scoped(value: &str) -> Option<(&str, &str, &str)> {
    let (room, rest) = value.split_once('/')?;
    let (cabinet, name) = rest.split_once('/')?;
    if !validate_segmented(room, cabinet) {
        return None;
    }
    if name.is_empty() {
        return None;
    }
    Some((room, cabinet, name))
}

/// 解析 "房间/设备名:端口/接口标识"。
pub fn split_device_scoped(value: &str) -> Option<(&str, &str, &str)> {
    let (device_path, port) = value.rsplit_once(':')?;
    let (room, device) = device_path.split_once('/')?;
    if !validate_segmented(room, device) {
        return None;
    }
    if port.is_empty() {
        return None;
    }
    Some((room, device, port))
}

/// 校验两段名称均非空（空段视为非法）
fn validate_segmented(head: &str, tail: &str) -> bool {
    !head.is_empty() && !tail.is_empty()
}

// inet/cidr 解析与地址归属判断的真身定义在 foims-common::net，
// 此处再导出保持 `crate::names::*` 调用路径稳定
pub use foims_common::net::{ip_in_cidr, parse_cidr, parse_inet};

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn 拆分房间内名称() {
        assert_eq!(split_room_scoped("机房A/机柜1"), Some(("机房A", "机柜1")));
        assert_eq!(split_room_scoped("机房A/"), None);
        assert_eq!(split_room_scoped("/机柜1"), None);
        assert_eq!(split_room_scoped("机柜1"), None);
    }

    #[test]
    fn 拆分机位路径() {
        assert_eq!(
            split_cabinet_scoped("机房A/机柜1/U1"),
            Some(("机房A", "机柜1", "U1"))
        );
        // 末段可包含斜杠以外的任意内容
        assert_eq!(
            split_cabinet_scoped("机房A/机柜1/配线架/2"),
            Some(("机房A", "机柜1", "配线架/2"))
        );
        assert_eq!(split_cabinet_scoped("机房A/机柜1"), None);
    }

    #[test]
    fn 拆分设备端口() {
        assert_eq!(
            split_device_scoped("机房A/交换机1:G1"),
            Some(("机房A", "交换机1", "G1"))
        );
        assert_eq!(split_device_scoped("机房A/交换机1"), None);
    }

    #[test]
    fn 解析网络地址() {
        assert_eq!(
            parse_inet("192.168.1.10"),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)))
        );
        assert_eq!(
            parse_inet("2001:db8::1/64").map(|ip| ip.is_ipv6()),
            Some(true)
        );
        assert_eq!(parse_inet("not-ip"), None);
    }

    #[test]
    fn 解析网段() {
        let (addr, prefix) = parse_cidr("10.20.0.0/16").unwrap();
        assert_eq!(addr, IpAddr::V4(Ipv4Addr::new(10, 20, 0, 0)));
        assert_eq!(prefix, 16);
        assert_eq!(parse_cidr("10.0.0.0/33"), None);
        assert_eq!(parse_cidr("10.0.0.0"), None);
    }

    #[test]
    fn 地址归属网段() {
        let net = IpAddr::V4(Ipv4Addr::new(10, 20, 0, 0));
        assert!(ip_in_cidr(
            IpAddr::V4(Ipv4Addr::new(10, 20, 255, 1)),
            net,
            16
        ));
        assert!(!ip_in_cidr(
            IpAddr::V4(Ipv4Addr::new(10, 21, 0, 1)),
            net,
            16
        ));
        // 跨族不匹配
        assert!(!ip_in_cidr(IpAddr::V6(Ipv6Addr::LOCALHOST), net, 16));
    }
}
