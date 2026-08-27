//! 数据模块定义：按业务模块组织导入导出的表集合。
//!
//! 设计取舍（模块化 vs 单表）：表间外键依赖紧密
//! （组织 → 房间 → 网络区域 → 网段 → 机柜 → 设备 → 线路 → 可视化），
//! 且一个业务模块由多张表构成（如设备模块含端口/网卡/接口/IP 表），
//! 按模块导入导出才能保证数据完整性、支撑部分导入时的依赖校验；
//! 按单表操作则要求使用者自行编排顺序与补齐关联，极易产生残缺数据。
//!
//! 注意：房间-网段绑定（room_networks）属于网络区域数据而非房间数据，
//! 单独导出网络区域模块时绑定关系完整包含。

/// 一个业务模块：名称与其包含的数据表（表顺序即模块内导入顺序）。
pub struct ModuleDef {
    pub name: &'static str,
    pub tables: &'static [&'static str],
}

/// 全部模块。数组顺序即全量导入的安全顺序（外键依赖拓扑序：
/// 组织 → 房间 → 网络区域 → 机柜 → 设备 → 线路 → 可视化）。
pub const MODULES: &[ModuleDef] = &[
    // 组织管理：组织结构模板 + 组织树（parent_id 自引用，导入时按依赖排序）+ 员工
    ModuleDef {
        name: "organization",
        tables: &["org_templates", "organizations", "employees"],
    },
    // 房间模块：房间 + 工位 + 信息点
    ModuleDef {
        name: "room",
        tables: &["rooms", "workstations", "net_outlets"],
    },
    // 网络区域模块：网络区域 + 网段 + 房间-网段绑定
    ModuleDef {
        name: "network",
        tables: &["network_regions", "network_cidrs", "room_networks"],
    },
    // 机柜模块：机柜 + 机位 + 配线架
    ModuleDef {
        name: "cabinet",
        tables: &["cabinets", "positions", "patch_panels"],
    },
    // 设备模块：设备模板 + 设备 + 端口/MAC/LLDP/网卡/接口 + IP
    ModuleDef {
        name: "device",
        tables: &[
            "device_templates",
            "devices",
            "device_ports",
            "device_macs",
            "device_lldps",
            "device_nics",
            "device_interfaces",
            "ips",
        ],
    },
    // 线路模块：跳接线路（端点为多态引用，导入前校验端点存在）
    ModuleDef {
        name: "cable-link",
        tables: &["cable_links"],
    },
    // 可视化模块：工位/元素/机柜布局 + 拓扑节点/连接/成员
    ModuleDef {
        name: "visualization",
        tables: &[
            "workstation_layouts",
            "element_layouts",
            "cabinet_layouts",
            "topology_nodes",
            "topology_connections",
            "topology_connection_members",
        ],
    },
];

/// 按名称查找模块定义。
pub fn find_module(name: &str) -> Option<&'static ModuleDef> {
    MODULES.iter().find(|m| m.name == name)
}

/// 表名所属的模块名（用于导入时校验表与模块的归属关系）。
pub fn table_module(table: &str) -> Option<&'static str> {
    MODULES
        .iter()
        .find(|m| m.tables.contains(&table))
        .map(|m| m.name)
}
