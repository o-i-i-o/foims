//! 内置 profile 文件清单（docs/SNMP模块重构方案.md §5.7）。
//!
//! 显式列出：加品牌 = 加一行 + 一个 TOML 文件。不用 build.rs、不用目录
//! 扫描、不用宏。顺序即描述通道（sysDescr）识别优先级：generic 是兜底层
//! 不参与识别；cisco/huawei/h3c 品牌族在前（词表精、判定优先），
//! 社区长尾按文件名序跟随。

pub(super) const FILES: &[(&str, &str)] = &[
    ("generic.toml", include_str!("generic.toml")),
    ("cisco.toml", include_str!("cisco.toml")),
    ("huawei.toml", include_str!("huawei.toml")),
    ("h3c.toml", include_str!("h3c.toml")),
    ("juniper.toml", include_str!("juniper.toml")),
    ("dell.toml", include_str!("dell.toml")),
    ("hp.toml", include_str!("hp.toml")),
    ("aruba.toml", include_str!("aruba.toml")),
    ("netgear.toml", include_str!("netgear.toml")),
    ("tp-link.toml", include_str!("tp-link.toml")),
    ("linksys.toml", include_str!("linksys.toml")),
    ("ubiquiti.toml", include_str!("ubiquiti.toml")),
    ("mikrotik.toml", include_str!("mikrotik.toml")),
    ("extreme.toml", include_str!("extreme.toml")),
    ("alcatel.toml", include_str!("alcatel.toml")),
    ("zyxel.toml", include_str!("zyxel.toml")),
    ("d-link.toml", include_str!("d-link.toml")),
    ("buffalo.toml", include_str!("buffalo.toml")),
    ("community-a-c.toml", include_str!("community-a-c.toml")),
    ("community-d-f.toml", include_str!("community-d-f.toml")),
    ("community-g-i.toml", include_str!("community-g-i.toml")),
    ("community-j-l.toml", include_str!("community-j-l.toml")),
    ("community-m-o.toml", include_str!("community-m-o.toml")),
    ("community-p-r.toml", include_str!("community-p-r.toml")),
    ("community-s-u.toml", include_str!("community-s-u.toml")),
    ("community-v-z.toml", include_str!("community-v-z.toml")),
];
