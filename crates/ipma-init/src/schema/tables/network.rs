//! 网络区域与网段（network_regions 等）表结构创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS network_regions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(20) NOT NULL UNIQUE,
            description TEXT,
            ipv4_cidrs CIDR[] DEFAULT '{}',
            ipv6_cidrs CIDR[] DEFAULT '{}',
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS network_cidrs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            network_region_id UUID NOT NULL REFERENCES network_regions(id),
            ipv4_cidr CIDR,
            ipv6_cidr CIDR,
            ipv4_gateway INET,
            ipv6_gateway INET,
            ipv4_dns INET[],
            ipv6_dns INET[],
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}
