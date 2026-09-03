//! 设备（devices）表结构创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS devices (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            hostname VARCHAR(100),
            device_type VARCHAR(30) NOT NULL,
            brand VARCHAR(50),
            model VARCHAR(100),
            serial_number VARCHAR(100),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            template_id UUID REFERENCES device_templates(id) ON DELETE SET NULL,
            seller VARCHAR(50), -- 销售商（采购渠道）
            location VARCHAR(100),
            snmp_version VARCHAR(3) DEFAULT 'v2c'
                CONSTRAINT chk_devices_snmp_version CHECK (snmp_version IN ('v1', 'v2c', 'v3')),
            -- SNMP 凭据列以密文落库（AES-GCM + base64：明文 +28 字节再编码），
            -- 列宽须容纳模型允许的最长明文加密结果（详见 check.rs 列宽契约）
            snmp_community VARCHAR(255),
            snmp_username VARCHAR(128),
            snmp_auth_protocol VARCHAR(10),
            snmp_auth_password VARCHAR(255),
            snmp_priv_protocol VARCHAR(10),
            snmp_priv_password VARCHAR(255),
            snmp_port INTEGER DEFAULT 161,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN (
                'desktop', 'laptop', 'printer', 'server', 'network_device', 'switch',
                'camera', 'phone', 'other'
            ))
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_devices_room_id ON devices(room_id)")
        .execute(pool)
        .await?;

    // 同房间设备名唯一：兜底 create/update 与导入路径的并发判重窗口
    //（预检为普通 SELECT，无该索引时并发双写可落两行同名设备）
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS uq_devices_room_name ON devices(room_id, name)")
        .execute(pool)
        .await?;

    Ok(())
}
