//! CSV 导入导出的表规格：列定义、名称引用与业务键。
//!
//! CSV 中一律使用业务名称（房间名、设备名等）代替 UUID 外键，
//! 导入时按业务键匹配已有行（存在则更新、不存在则插入，主键 UUID
//! 由数据库生成）。复合引用使用 "/" 连接（如设备引用 "机房/设备名"），
//! 因此参与复合引用的名称列不允许包含 "/"，导入时校验。
//!
//! 三类列：
//! - [`Col::Plain`]：普通业务列，CSV 列名与数据库列名一致；
//! - [`Col::Ref`]：外键引用列，CSV 中为名称（路径），导入时解析为 UUID
//!   写入指定数据库列；
//! - [`Col::Info`]：伴随列，仅用于人工阅读或供引用解析读取上下文
//!   （如网段引用需要区域名），不写入数据库。

/// CSV 列的取值方式。
pub enum Col {
    /// 普通业务列：CSV 列名与数据库列名一致，值原样导入导出。
    Plain(&'static str),
    /// 外键引用列：CSV 中为名称（或名称路径），导入时解析为 UUID 写入 `db` 列。
    Ref {
        csv: &'static str,
        db: &'static str,
        target: Target,
    },
    /// 伴随列：不写库，供引用目标读取上下文或导出时补充展示。
    Info(&'static str),
}

/// 名称引用的解析方式。各变体从行内读取的列在其文档中说明，
/// 规格表中各表的列名与之保持一致。
pub enum Target {
    /// 本列值即目标表的全局唯一名称（rooms、net_outlets、org_templates、
    /// device_templates、network_regions）。
    ByName {
        table: &'static str,
        name_col: &'static str,
    },
    /// 本列值为组织全路径（自根起 "/" 连接），空串表示根（parent_id NULL）。
    OrgPath,
    /// 本列值为网段名，与行内 `region_name` 伴随列组成 (区域, 网段) 业务键
    /// 指向 network_cidrs（该表名称不唯一，须靠区域消歧）。
    Cidr,
    /// 本列值为 "房间/名称"，指向 workstations 或 cabinets。
    RoomScoped { table: &'static str },
    /// 本列值为 "房间/机柜/机位名"，指向 positions。
    Position,
    /// 本列值为 "房间/设备名"，指向 devices。
    Device,
    /// 行内 `device` 伴随/引用列 + 本列网卡名，指向 device_nics
    /// （仅 device_interfaces 表使用，其行内本就有 device 列）。
    DeviceNic,
    /// 行内 `device` 伴随列 + 本列接口名，指向 device_interfaces
    /// （仅 ips 表使用，该表无 device_id 列）。
    DeviceInterface,
    /// 本列值自编码为 "房间/设备名:端口号"，指向 device_ports。
    DevicePort,
    /// 跳接线路多态端点：同表 `type_col` 列给出端点类型，本列值为该类型
    /// 的名称路径（device_port="房间/设备:端口号"、device_interface=
    /// "房间/设备:接口名"、net_outlet="信息点名"、patch_panel="房间/机柜/配线架名"）。
    Endpoint { type_col: &'static str },
    /// 行内 `source_device`/`target_device`/`connection_type` 伴随列组合，
    /// 指向 topology_connections（仅成员表使用；物理连接无唯一约束，
    /// 匹配 earliest 一条，尽力而为）。
    TopologyConnection,
}

/// 一张表的 CSV 规格。
pub struct TableSpec {
    pub table: &'static str,
    /// CSV 列顺序（即表头顺序）。
    pub columns: &'static [Col],
    /// 业务键的数据库列，导入时据此匹配已有行与批内去重。
    /// 可空列（如 positions.cabinet_id、organizations.parent_id）
    /// 匹配时使用 IS NOT DISTINCT FROM。
    pub key: &'static [&'static str],
}

/// 全部表的规格。顺序即同一模块内的导入顺序（被引用表在前）。
pub const TABLE_SPECS: &[TableSpec] = &[
    // ---------- 组织模块 ----------
    TableSpec {
        table: "org_templates",
        columns: &[
            Col::Plain("name"),
            Col::Plain("levels"),
            Col::Plain("icons"),
            Col::Plain("description"),
        ],
        key: &["name"],
    },
    TableSpec {
        table: "organizations",
        columns: &[
            Col::Plain("name"),
            Col::Ref {
                csv: "parent_path",
                db: "parent_id",
                target: Target::OrgPath,
            },
            Col::Ref {
                csv: "template_name",
                db: "template_id",
                target: Target::ByName {
                    table: "org_templates",
                    name_col: "name",
                },
            },
            Col::Plain("type_path"),
            Col::Plain("description"),
        ],
        key: &["parent_id", "name"],
    },
    // 员工挂在组织节点下（org_id 非空，业务键 = 组织 + 姓名）
    TableSpec {
        table: "employees",
        columns: &[
            Col::Ref {
                csv: "org_path",
                db: "org_id",
                target: Target::OrgPath,
            },
            Col::Plain("name"),
            Col::Plain("gender"),
            Col::Plain("phone"),
            Col::Plain("email"),
            Col::Plain("hire_date"),
        ],
        key: &["org_id", "name"],
    },
    // ---------- 房间模块 ----------
    TableSpec {
        table: "rooms",
        columns: &[
            Col::Plain("name"),
            Col::Plain("room_type"),
            Col::Ref {
                csv: "org_path",
                db: "org_id",
                target: Target::OrgPath,
            },
            Col::Plain("description"),
        ],
        key: &["name"],
    },
    TableSpec {
        table: "workstations",
        columns: &[
            Col::Ref {
                csv: "room_name",
                db: "room_id",
                target: Target::ByName {
                    table: "rooms",
                    name_col: "name",
                },
            },
            Col::Plain("name"),
            Col::Plain("manager"),
            Col::Plain("description"),
        ],
        key: &["room_id", "name"],
    },
    TableSpec {
        table: "net_outlets",
        columns: &[
            Col::Plain("name"),
            Col::Ref {
                csv: "room_name",
                db: "room_id",
                target: Target::ByName {
                    table: "rooms",
                    name_col: "name",
                },
            },
        ],
        key: &["name"],
    },
    // ---------- 网络区域模块（含房间-网段绑定） ----------
    TableSpec {
        table: "network_regions",
        columns: &[
            Col::Plain("name"),
            Col::Plain("description"),
            Col::Plain("ipv4_cidrs"),
            Col::Plain("ipv6_cidrs"),
        ],
        key: &["name"],
    },
    TableSpec {
        table: "network_cidrs",
        columns: &[
            Col::Ref {
                csv: "region_name",
                db: "network_region_id",
                target: Target::ByName {
                    table: "network_regions",
                    name_col: "name",
                },
            },
            Col::Plain("name"),
            Col::Plain("ipv4_cidr"),
            Col::Plain("ipv6_cidr"),
            Col::Plain("ipv4_gateway"),
            Col::Plain("ipv6_gateway"),
            Col::Plain("ipv4_dns"),
            Col::Plain("ipv6_dns"),
            Col::Plain("description"),
        ],
        key: &["network_region_id", "name"],
    },
    TableSpec {
        table: "room_networks",
        columns: &[
            Col::Ref {
                csv: "room_name",
                db: "room_id",
                target: Target::ByName {
                    table: "rooms",
                    name_col: "name",
                },
            },
            Col::Info("region_name"),
            Col::Ref {
                csv: "network_name",
                db: "network_id",
                target: Target::Cidr,
            },
        ],
        key: &["room_id", "network_id"],
    },
    // ---------- 机柜模块 ----------
    TableSpec {
        table: "cabinets",
        columns: &[
            Col::Ref {
                csv: "room_name",
                db: "room_id",
                target: Target::ByName {
                    table: "rooms",
                    name_col: "name",
                },
            },
            Col::Plain("name"),
            Col::Plain("capacity"),
            Col::Plain("description"),
        ],
        key: &["room_id", "name"],
    },
    TableSpec {
        table: "positions",
        columns: &[
            Col::Ref {
                csv: "cabinet",
                db: "cabinet_id",
                target: Target::RoomScoped { table: "cabinets" },
            },
            Col::Plain("name"),
            Col::Plain("start_u"),
            Col::Plain("end_u"),
            Col::Plain("description"),
        ],
        key: &["cabinet_id", "name"],
    },
    TableSpec {
        table: "patch_panels",
        columns: &[
            Col::Ref {
                csv: "cabinet",
                db: "cabinet_id",
                target: Target::RoomScoped { table: "cabinets" },
            },
            Col::Plain("name"),
        ],
        key: &["cabinet_id", "name"],
    },
    // ---------- 设备模块 ----------
    TableSpec {
        table: "device_templates",
        columns: &[
            Col::Plain("name"),
            Col::Plain("device_type"),
            Col::Plain("brand"),
            Col::Plain("model"),
            Col::Plain("description"),
        ],
        key: &["name"],
    },
    TableSpec {
        table: "devices",
        columns: &[
            Col::Ref {
                csv: "room_name",
                db: "room_id",
                target: Target::ByName {
                    table: "rooms",
                    name_col: "name",
                },
            },
            Col::Plain("name"),
            Col::Plain("hostname"),
            Col::Plain("device_type"),
            Col::Plain("brand"),
            Col::Plain("model"),
            Col::Plain("serial_number"),
            Col::Ref {
                csv: "workstation",
                db: "workstation_id",
                target: Target::RoomScoped {
                    table: "workstations",
                },
            },
            Col::Ref {
                csv: "position",
                db: "position_id",
                target: Target::Position,
            },
            Col::Ref {
                csv: "template_name",
                db: "template_id",
                target: Target::ByName {
                    table: "device_templates",
                    name_col: "name",
                },
            },
            Col::Plain("seller"),
            Col::Plain("location"),
            Col::Plain("snmp_version"),
            Col::Plain("snmp_community"),
            Col::Plain("snmp_username"),
            Col::Plain("snmp_auth_protocol"),
            Col::Plain("snmp_auth_password"),
            Col::Plain("snmp_priv_protocol"),
            Col::Plain("snmp_priv_password"),
            Col::Plain("snmp_port"),
            Col::Plain("description"),
        ],
        key: &["room_id", "name"],
    },
    TableSpec {
        table: "device_ports",
        columns: &[
            Col::Ref {
                csv: "device",
                db: "device_id",
                target: Target::Device,
            },
            Col::Plain("port_number"),
            Col::Plain("port_name"),
            Col::Plain("port_type"),
            Col::Plain("vlan_id"),
            Col::Plain("status"),
            Col::Plain("speed"),
            Col::Plain("description"),
        ],
        key: &["device_id", "port_number"],
    },
    TableSpec {
        table: "device_macs",
        columns: &[
            Col::Ref {
                csv: "device",
                db: "device_id",
                target: Target::Device,
            },
            Col::Plain("ip_address"),
            Col::Plain("mac_address"),
            Col::Plain("interface"),
            Col::Plain("vlan_id"),
        ],
        key: &["device_id", "ip_address"],
    },
    TableSpec {
        table: "device_lldps",
        columns: &[
            Col::Ref {
                csv: "device",
                db: "device_id",
                target: Target::Device,
            },
            Col::Plain("local_port"),
            Col::Plain("neighbor_chassis_id"),
            Col::Plain("neighbor_port_id"),
            Col::Plain("neighbor_port_desc"),
            Col::Plain("neighbor_sys_name"),
            Col::Plain("neighbor_sys_desc"),
        ],
        key: &["device_id", "local_port"],
    },
    TableSpec {
        table: "device_nics",
        columns: &[
            Col::Ref {
                csv: "device",
                db: "device_id",
                target: Target::Device,
            },
            Col::Plain("name"),
            Col::Plain("card_type"),
            Col::Plain("description"),
            Col::Plain("sort_order"),
        ],
        key: &["device_id", "name"],
    },
    TableSpec {
        table: "device_interfaces",
        columns: &[
            Col::Ref {
                csv: "device",
                db: "device_id",
                target: Target::Device,
            },
            Col::Ref {
                csv: "nic_name",
                db: "nic_id",
                target: Target::DeviceNic,
            },
            Col::Plain("name"),
            Col::Plain("physical_type"),
            Col::Plain("interface_role"),
            Col::Plain("mac_address"),
            Col::Plain("vlan_id"),
            Col::Plain("description"),
            Col::Plain("sort_order"),
        ],
        key: &["device_id", "name"],
    },
    TableSpec {
        table: "ips",
        columns: &[
            Col::Info("device"),
            Col::Ref {
                csv: "interface_name",
                db: "device_interface_id",
                target: Target::DeviceInterface,
            },
            Col::Info("region_name"),
            Col::Ref {
                csv: "network_name",
                db: "network_id",
                target: Target::Cidr,
            },
            Col::Plain("ip_address"),
            Col::Plain("ip_version"),
            Col::Plain("description"),
            Col::Plain("status"),
        ],
        key: &["ip_address"],
    },
    // ---------- 线路模块 ----------
    TableSpec {
        table: "cable_links",
        columns: &[
            Col::Plain("a_endpoint_type"),
            Col::Ref {
                csv: "a_endpoint",
                db: "a_endpoint_id",
                target: Target::Endpoint {
                    type_col: "a_endpoint_type",
                },
            },
            Col::Plain("b_endpoint_type"),
            Col::Ref {
                csv: "b_endpoint",
                db: "b_endpoint_id",
                target: Target::Endpoint {
                    type_col: "b_endpoint_type",
                },
            },
            Col::Plain("link_type"),
            Col::Plain("cable_label"),
            Col::Plain("length_m"),
            Col::Plain("tested"),
        ],
        key: &["a_endpoint_id", "b_endpoint_id"],
    },
    // ---------- 可视化模块 ----------
    TableSpec {
        table: "workstation_layouts",
        columns: &[
            Col::Ref {
                csv: "workstation",
                db: "workstation_id",
                target: Target::RoomScoped {
                    table: "workstations",
                },
            },
            Col::Ref {
                csv: "room_name",
                db: "room_id",
                target: Target::ByName {
                    table: "rooms",
                    name_col: "name",
                },
            },
            Col::Plain("x"),
            Col::Plain("y"),
            Col::Plain("width"),
            Col::Plain("height"),
            Col::Plain("rotation"),
        ],
        key: &["workstation_id"],
    },
    TableSpec {
        table: "element_layouts",
        columns: &[
            Col::Ref {
                csv: "room_name",
                db: "room_id",
                target: Target::ByName {
                    table: "rooms",
                    name_col: "name",
                },
            },
            Col::Plain("element_type"),
            Col::Plain("x"),
            Col::Plain("y"),
            Col::Plain("width"),
            Col::Plain("height"),
            Col::Plain("rotation"),
        ],
        key: &["room_id", "element_type"],
    },
    TableSpec {
        table: "cabinet_layouts",
        columns: &[
            Col::Ref {
                csv: "cabinet",
                db: "cabinet_id",
                target: Target::RoomScoped { table: "cabinets" },
            },
            Col::Plain("x"),
            Col::Plain("y"),
            Col::Plain("width"),
            Col::Plain("height"),
            Col::Plain("rotation"),
        ],
        key: &["cabinet_id"],
    },
    TableSpec {
        table: "topology_nodes",
        columns: &[
            Col::Ref {
                csv: "device",
                db: "device_id",
                target: Target::Device,
            },
            Col::Plain("x"),
            Col::Plain("y"),
            Col::Plain("width"),
            Col::Plain("height"),
        ],
        key: &["device_id"],
    },
    TableSpec {
        table: "topology_connections",
        columns: &[
            Col::Ref {
                csv: "source_device",
                db: "source_device_id",
                target: Target::Device,
            },
            Col::Ref {
                csv: "target_device",
                db: "target_device_id",
                target: Target::Device,
            },
            Col::Ref {
                csv: "source_port",
                db: "source_device_port_id",
                target: Target::DevicePort,
            },
            Col::Ref {
                csv: "target_port",
                db: "target_device_port_id",
                target: Target::DevicePort,
            },
            Col::Plain("label"),
            Col::Plain("auto_discovered"),
            Col::Plain("connection_type"),
        ],
        key: &[
            "source_device_id",
            "target_device_id",
            "connection_type",
            "label",
        ],
    },
    TableSpec {
        table: "topology_connection_members",
        columns: &[
            Col::Info("source_device"),
            Col::Info("target_device"),
            Col::Info("connection_type"),
            Col::Ref {
                csv: "connection",
                db: "connection_id",
                target: Target::TopologyConnection,
            },
            Col::Ref {
                csv: "device",
                db: "device_id",
                target: Target::Device,
            },
            Col::Ref {
                csv: "port",
                db: "device_port_id",
                target: Target::DevicePort,
            },
            Col::Plain("side"),
        ],
        key: &["connection_id", "device_port_id"],
    },
];

/// 按表名查找规格。
pub fn find_spec(table: &str) -> Option<&'static TableSpec> {
    TABLE_SPECS.iter().find(|s| s.table == table)
}

/// 按表头集合识别表（单个 CSV 导入时不比对文件名，仅凭表头匹配）。
/// 各表表头集合互不相同，无歧义。
pub fn spec_by_header(headers: &[String]) -> Option<&'static TableSpec> {
    TABLE_SPECS.iter().find(|spec| {
        let spec_headers: std::collections::HashSet<&str> =
            spec.columns.iter().map(Col::csv_name).collect();
        let got: std::collections::HashSet<&str> = headers.iter().map(String::as_str).collect();
        spec_headers == got
    })
}

/// 表的 CSV 表头（有序）。
pub fn headers(spec: &TableSpec) -> Vec<&'static str> {
    spec.columns.iter().map(Col::csv_name).collect()
}

/// 参与复合引用（"/" 路径）的名称列：导入时禁止包含 "/"。
pub const COMPOSITE_NAME_TABLES: &[(&str, &str)] = &[
    ("rooms", "name"),
    ("workstations", "name"),
    ("cabinets", "name"),
    ("positions", "name"),
    ("devices", "name"),
    ("organizations", "name"),
];

impl Col {
    /// CSV 列名。
    pub fn csv_name(&self) -> &'static str {
        match self {
            Col::Plain(col) | Col::Info(col) => col,
            Col::Ref { csv, .. } => csv,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 各表表头集合必须互不相同，否则按表头识别表会产生歧义。
    /// 修改规格时若触发本测试，需为新表补充区分列。
    #[test]
    fn 表头集合全局唯一() {
        let mut seen: HashSet<Vec<&str>> = HashSet::new();
        for spec in TABLE_SPECS {
            let mut h = headers(spec);
            h.sort_unstable();
            assert!(seen.insert(h), "表 {} 的表头集合与其他表重复", spec.table);
        }
    }

    /// 各表的业务键列必须能从列规格解析出值（普通列或引用列；
    /// 伴随列不写库，不能作为业务键）。
    #[test]
    fn 业务键列均有来源() {
        for spec in TABLE_SPECS {
            for key in spec.key {
                let from_col = spec.columns.iter().any(|c| match c {
                    Col::Plain(col) => col == key,
                    Col::Ref { db, .. } => db == key,
                    Col::Info(_) => false,
                });
                assert!(
                    from_col,
                    "表 {} 的业务键列 {} 未在列规格中定义",
                    spec.table, key
                );
            }
        }
    }
}
