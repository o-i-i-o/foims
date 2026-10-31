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
            -- IPMI 带外管理凭据（RMCP+ 会话）：username 与 password 均非空即
            -- 视为「已配置」，不设独立开关列。password 以密文落库（AES-GCM +
            -- base64：明文 +28 字节再编码），列宽须容纳模型允许的最长明文
            -- 加密结果（IPMI 规范口令 ≤20 字节，详见 check.rs 列宽契约）。
            -- 端口/权限/算法受 ruipmi 约束固定：会话端口恒 623（IPMI_LANPLUS_PORT
            -- 硬编码），特权固定 Administrator，故 ipmi_port 仅承载展示语义、
            -- 采集侧不使用；ipmi_auth_algo 选择 RAKP 完整性/加密套件（sha256=17、
            -- sha1=3），老固件兼容回落用 sha1。
            ipmi_username VARCHAR(62),
            ipmi_password VARCHAR(160),
            ipmi_port INTEGER DEFAULT 623,
            ipmi_auth_algo VARCHAR(10) DEFAULT 'sha256'
                CONSTRAINT chk_devices_ipmi_auth_algo CHECK (ipmi_auth_algo IN ('sha1', 'sha256')),
            -- 设备额定功耗（瓦，可空=未记录）；机柜/房间功率容量校验的求和来源
            power_watts INTEGER
                CONSTRAINT chk_devices_power_watts CHECK (power_watts IS NULL OR power_watts >= 0),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN (
                'desktop', 'laptop', 'printer', 'server', 'network_device', 'switch',
                'camera', 'phone', 'other'
            )),
            -- 一设备一个空间挂载点：工位与机位互斥（同时挂两边时 room_id
            -- 无法同时满足两个父级房间，触发器会随机报错，必须结构上排掉）
            CONSTRAINT chk_devices_single_location CHECK (
                workstation_id IS NULL OR position_id IS NULL
            )
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
