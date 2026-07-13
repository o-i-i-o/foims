mod cabinets;
mod cable_links;
mod device_interfaces;
mod device_templates;
mod devices;
mod element;
mod encryption;
mod indexes;
mod ips;
mod logs;
mod net_outlets;
mod network;
mod notifications;
mod org_templates;
mod organizations;
mod rooms;
mod switch_ports;
mod system;
mod tokens;
mod topology;
mod triggers;
mod users;
mod views;
mod workstations;

pub async fn create_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
        .execute(pool)
        .await?;

    // 无外键依赖的基础表
    users::create(pool).await?;
    encryption::create(pool).await?;
    system::create(pool).await?;
    org_templates::create(pool).await?;
    device_templates::create(pool).await?;

    // 网络（network_regions → network_cidrs）
    network::create(pool).await?;

    // 组织（引用 org_templates、自引用）
    organizations::create(pool).await?;

    // 房间（引用 organizations、network_cidrs）
    rooms::create(pool).await?;

    // 机柜/机位（引用 rooms → cabinets → positions）
    cabinets::create(pool).await?;

    // 工位（引用 rooms）
    workstations::create(pool).await?;

    // 布局元素（引用 rooms）
    element::create(pool).await?;

    // 设备（引用 workstations/positions/device_templates/net_outlets）
    devices::create(pool).await?;

    // 交换机端口/MAC/LLDP（引用 devices）
    switch_ports::create(pool).await?;

    // 设备三层接口（引用 devices）
    device_interfaces::create(pool).await?;

    // 信息点（引用 rooms/cabinets）
    net_outlets::create(pool).await?;

    // 物理链路（引用 switch_ports/net_outlets/device_interfaces 由触发器校验）
    cable_links::create(pool).await?;

    // IP（引用 device_interfaces/devices/network_cidrs）
    ips::create(pool).await?;

    // 拓扑（引用 devices/switch_ports）
    topology::create(pool).await?;

    // 日志/令牌/通知（引用 users）
    logs::create(pool).await?;
    tokens::create(pool).await?;
    notifications::create(pool).await?;

    indexes::create(pool).await?;
    views::create(pool).await?;
    triggers::create(pool).await?;

    // 角色级超时配置（永久生效，所有新连接自动应用）
    sqlx::query("ALTER ROLE CURRENT_USER SET statement_timeout = '30s'")
        .execute(pool)
        .await?;
    sqlx::query("ALTER ROLE CURRENT_USER SET lock_timeout = '5s'")
        .execute(pool)
        .await?;

    Ok(())
}
