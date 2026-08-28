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

use std::net::IpAddr;

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

/// 解析 inet 值（可选 /nn 后缀，按 PG 语义忽略掩码只取地址）。
pub fn parse_inet(value: &str) -> Option<IpAddr> {
    let addr = value.split('/').next()?;
    addr.parse::<IpAddr>().ok()
}

/// 解析 cidr 值，返回 (网络地址, 前缀长度)。
pub fn parse_cidr(value: &str) -> Option<(IpAddr, u8)> {
    let (addr, prefix) = value.split_once('/')?;
    let addr: IpAddr = addr.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    let max = match addr {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    };
    if prefix > max {
        return None;
    }
    Some((addr, prefix))
}

/// 地址是否属于网段（含网络地址与广播地址）。
pub fn ip_in_cidr(ip: IpAddr, cidr_addr: IpAddr, prefix: u8) -> bool {
    match (ip, cidr_addr) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => {
            if prefix > 32 {
                return false;
            }
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            (u32::from(ip) & mask) == (u32::from(net) & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(net)) => {
            if prefix > 128 {
                return false;
            }
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            (u128::from(ip) & mask) == (u128::from(net) & mask)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

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
