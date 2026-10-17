//! 设备 SNMP 采集域（snmp_arp_entries、device_lldps）表结构创建。
//!
//! snmp_arp_entries 存储设备 SNMP 观测到的 ARP/FDB 条目（IP ↔ MAC ↔
//! 学习端口 ↔ VLAN），device_lldps 存储设备 LLDP 邻居发现条目；
//! 二者同源于设备 SNMP 采集，与端口台账表 device_interfaces
//! 的 mac_address/vlan_id 配置语义无重复。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS snmp_arp_entries (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            ip_address INET NOT NULL,
            mac_address VARCHAR(20) NOT NULL,
            interface VARCHAR(50),
            vlan_id INTEGER,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, ip_address)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_lldps (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            local_port VARCHAR(50) NOT NULL,
            neighbor_chassis_id VARCHAR(100),
            neighbor_port_id VARCHAR(100),
            neighbor_port_desc VARCHAR(255),
            neighbor_sys_name VARCHAR(255),
            neighbor_sys_desc TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, local_port)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
