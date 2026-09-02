//! IP（ips）表结构创建。
//!
//! MAC 地址归属于 device_interfaces（网口），主机名归属于 devices，
//! 本表仅存储 IP 本身及其网段归属，设备经 device_interface_id 关联。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS ips (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_interface_id UUID NOT NULL REFERENCES device_interfaces(id) ON DELETE CASCADE,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            ip_address INET NOT NULL,
            ip_version SMALLINT NOT NULL DEFAULT 4,
            description TEXT,
            status VARCHAR(20) NOT NULL DEFAULT 'active',
            last_seen TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_ips_ip_address UNIQUE (ip_address)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
