mod cabinets;
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
mod switches;
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

    // 设备（引用 workstations/positions/device_templates，device_port_id 延迟添加）
    devices::create(pool).await?;

    // 交换机端口/MAC/LLDP（引用 devices）
    switches::create(pool).await?;

    // 信息点（引用 rooms/cabinets，device_port_id 延迟添加）
    net_outlets::create(pool).await?;

    // IP（引用 workstations/positions/device_ports/devices/network_cidrs）
    ips::create(pool).await?;

    // 拓扑（引用 devices/device_ports）
    topology::create(pool).await?;

    // 日志/令牌/通知（引用 users）
    logs::create(pool).await?;
    tokens::create(pool).await?;
    notifications::create(pool).await?;

    // 延迟外键（解决 devices ↔ device_ports 循环依赖）
    devices::add_foreign_keys(pool).await?;
    net_outlets::add_foreign_keys(pool).await?;

    indexes::create(pool).await?;
    views::create(pool).await?;
    triggers::create(pool).await?;

    Ok(())
}
