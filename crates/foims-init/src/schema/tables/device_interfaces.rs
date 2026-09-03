//! 统一端口/网口表（device_interfaces）结构创建。
//!
//! 设备端口（原 device_ports）与设备网口已合并：`port_type`/`status`/
//! `speed` 为网络设备端口的二层属性（SNMP 维护），`device_managed`
//! 标记网口是否在设备编辑模态框中展示维护。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_interfaces (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            nic_id UUID REFERENCES device_nics(id) ON DELETE SET NULL,
            name VARCHAR(50) NOT NULL,
            physical_type VARCHAR(20) NOT NULL DEFAULT 'rj45' CHECK (physical_type IN (
                'rj45', 'sfp', 'sfp_plus', 'sfp28', 'qsfp_plus', 'qsfp28', 'wifi', 'virtual', 'other'
            )),
            interface_role VARCHAR(20) NOT NULL DEFAULT 'business' CHECK (interface_role IN (
                'management', 'business', 'loopback', 'uplink', 'other'
            )),
            mac_address VARCHAR(20),
            vlan_id INTEGER,
            description TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            port_type VARCHAR(20) NOT NULL DEFAULT 'access' CHECK (port_type IN (
                'access', 'trunk', 'hybrid', 'uplink', 'stack', 'console'
            )),
            status VARCHAR(20) NOT NULL DEFAULT 'up'
                CONSTRAINT chk_device_interfaces_status CHECK (status IN ('up', 'down', 'admin-down')),
            speed VARCHAR(20),
            trunk_id INTEGER,
            device_managed BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, name)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
