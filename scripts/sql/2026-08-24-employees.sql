-- 2026-08-24 员工管理功能结构变更（现有部署直接执行；
-- 全新部署由 foims-init 建表代码自动完成，无需执行本文件）
--
-- 1. 新增员工表：员工挂在组织节点下，供工位管理人选择与邮件通知使用
CREATE TABLE IF NOT EXISTS employees (
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
);

CREATE INDEX IF NOT EXISTS idx_employees_org_id ON employees(org_id);

-- 2. 工位增加管理人员工引用（保留原 manager 文本列作为显示名兼容旧数据）
ALTER TABLE workstations
    ADD COLUMN IF NOT EXISTS manager_employee_id UUID REFERENCES employees(id) ON DELETE SET NULL;

-- 3. 等保三级密码策略：密码变更时间 + 历史密码（重复使用检查）
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS password_changed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW();

CREATE TABLE IF NOT EXISTS password_history (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    password_hash VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_password_history_user_id ON password_history(user_id, created_at DESC);
