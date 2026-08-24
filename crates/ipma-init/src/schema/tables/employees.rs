//! 员工（employees）表结构创建。
//!
//! 员工挂在组织节点下（组织模板不变），供工位管理人选择、
//! 设备分配 IP 后的邮件通知等场景使用。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS employees (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name VARCHAR(50) NOT NULL,
            gender VARCHAR(10) NOT NULL DEFAULT 'unknown',
            phone VARCHAR(20),
            email VARCHAR(100),
            hire_date DATE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_employees_org_name UNIQUE NULLS NOT DISTINCT (org_id, name),
            CONSTRAINT ck_employees_gender CHECK (gender IN ('male', 'female', 'unknown'))
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_employees_org_id ON employees(org_id)")
        .execute(pool)
        .await?;

    Ok(())
}
