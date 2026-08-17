//! 设备网卡（device_nics）表结构创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_nics (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            name VARCHAR(50) NOT NULL,
            card_type VARCHAR(20) NOT NULL DEFAULT 'pcie' CHECK (card_type IN (
                'pcie', 'onboard', 'usb', 'virtual', 'wwan', 'wifi', 'other'
            )),
            description TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, name)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
