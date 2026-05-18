# 门元素保存到 workstation_layouts 表的修改说明

## 修改概述

本次修改实现了将工位可视化中的门元素（方向标志物）保存到 `workstation_layouts` 表的功能。

## 主要变更

### 1. 数据库表结构重构

#### workstation_layouts 表结构变更

**旧结构：**
```sql
CREATE TABLE workstation_layouts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    workstation_id UUID NOT NULL REFERENCES workstations(id) ON DELETE CASCADE,
    x INTEGER NOT NULL DEFAULT 0,
    y INTEGER NOT NULL DEFAULT 0,
    width INTEGER NOT NULL DEFAULT 160,
    height INTEGER NOT NULL DEFAULT 160,
    rotation INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(workstation_id)
);
```

**新结构：**
```sql
CREATE TABLE workstation_layouts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    element_id UUID NOT NULL,
    element_type VARCHAR(20) NOT NULL DEFAULT 'workstation',
    x INTEGER NOT NULL DEFAULT 0,
    y INTEGER NOT NULL DEFAULT 0,
    width INTEGER NOT NULL DEFAULT 160,
    height INTEGER NOT NULL DEFAULT 160,
    rotation INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(room_id, element_id)
);
```

**变更说明：**
- 移除 `workstation_id` 字段及其外键约束
- 新增 `room_id` 字段，关联到房间
- 新增 `element_id` 字段，存储元素的UUID（可以是工位ID或门的静态UUID）
- 新增 `element_type` 字段，区分元素类型（'workstation' 或 'door'）
- 唯一约束从 `workstation_id` 改为 `(room_id, element_id)` 组合

### 2. 后端代码修改

#### src/init/schema.rs
- 更新 `workstation_layouts` 表创建语句
- 更新索引：`idx_workstation_layouts_room_id` 和 `idx_workstation_layouts_element_type`
- 添加迁移函数 `migrate_workstation_layouts_structure()` 处理现有数据库升级
- 添加 `info` 宏导入用于日志记录

#### src/resource/drawing.rs

**保存布局逻辑（save_layout函数）：**
```rust
// 旧代码：跳过年元素
if item.element_type == "door" {
    continue;
}

// 新代码：支持保存门元素
let element_type = if item.element_type == "door" {
    "door"
} else {
    "workstation"
};

sqlx::query(
    "INSERT INTO workstation_layouts (room_id, element_id, element_type, x, y, width, height, rotation) 
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
     ON CONFLICT (room_id, element_id) 
     DO UPDATE SET ..."
)
.bind(room_id)
.bind(item.id)
.bind(element_type)
...
```

**查询布局逻辑（get_layout函数）：**
```rust
// 旧代码：只查询工位
SELECT wl.workstation_id, ...
FROM workstation_layouts wl
JOIN workstations w ON wl.workstation_id = w.id
WHERE w.room_id = $1

// 新代码：查询所有元素（包括门）
SELECT element_id, element_type, ...
FROM workstation_layouts
WHERE room_id = $1
```

**删除布局逻辑（delete_layout函数）：**
```rust
// 旧代码
DELETE FROM workstation_layouts WHERE workstation_id IN (SELECT id FROM workstations WHERE room_id = $1)

// 新代码
DELETE FROM workstation_layouts WHERE room_id = $1
```

#### src/resource/workstation.rs
更新删除工位时的布局清理逻辑：
```rust
// 旧代码
DELETE FROM workstation_layouts WHERE workstation_id = $1

// 新代码
DELETE FROM workstation_layouts WHERE element_id = $1 AND element_type = 'workstation'
```

#### src/init/check.rs
更新表结构检查的列定义：
```rust
columns.insert(
    "workstation_layouts",
    vec![
        "id",
        "room_id",        // 新增
        "element_id",     // 新增
        "element_type",   // 新增
        "x",
        "y",
        "width",
        "height",
        "rotation",
        "created_at",
        "updated_at",
    ],
);
```

### 3. 前端代码

前端代码无需修改，已正确使用门的静态UUID：
- 文件：`web/static/js/modules/visualization/SVGRenderer.js`
- 门的UUID：`00000000-0000-0000-0000-000000000001`（符合RFC 4122标准）

## 数据迁移

系统会自动执行迁移：
1. 检测旧表结构是否存在 `workstation_id` 字段
2. 如果存在，创建备份表 `workstation_layouts_backup`
3. 删除旧表
4. 重新创建新表结构
5. 记录迁移版本到 `schema_migrations` 表

**注意：** 迁移会清空现有的布局数据，建议在迁移前手动备份重要数据。

## API 行为变更

### 保存布局 API: POST /api/resources/layouts

**请求体示例：**
```json
{
  "type": "workstation",
  "room_id": "uuid-of-room",
  "layout": [
    {
      "id": "00000000-0000-0000-0000-000000000001",
      "element_type": "door",
      "position": {
        "x": 50,
        "y": 100,
        "width": 40,
        "height": 80,
        "rotation": 0
      }
    },
    {
      "id": "workstation-uuid",
      "element_type": "workstation",
      "position": {
        "x": 150,
        "y": 100,
        "width": 160,
        "height": 160,
        "rotation": 0
      }
    }
  ]
}
```

### 查询布局 API: GET /api/resources/layouts/{room_id}

**响应示例：**
```json
{
  "success": true,
  "data": [
    {
      "id": "00000000-0000-0000-0000-000000000001",
      "element_type": "door",
      "position": {
        "x": 50,
        "y": 100,
        "width": 40,
        "height": 80,
        "rotation": 0
      }
    },
    {
      "id": "workstation-uuid",
      "element_type": "workstation",
      "position": {
        "x": 150,
        "y": 100,
        "width": 160,
        "height": 160,
        "rotation": 0
      }
    }
  ]
}
```

## 测试建议

1. **初始化测试：** 重启应用，验证迁移是否成功执行
2. **保存测试：** 在工位可视化界面拖动门元素并保存布局
3. **查询测试：** 刷新页面，验证门元素位置是否正确恢复
4. **删除测试：** 删除房间布局，验证门元素和工位布局都被清除
5. **工位删除测试：** 删除工位，验证只删除该工位的布局，不影响门元素

## 注意事项

1. 门的静态UUID必须使用标准格式：`00000000-0000-0000-0000-000000000001`
2. 迁移会清空现有布局数据，生产环境需谨慎操作
3. 新表结构支持扩展更多元素类型（如窗户、柱子等）
4. `element_type` 字段目前支持：'workstation' 和 'door'

## 相关文件清单

- `/root/ipma/src/init/schema.rs` - 表结构定义和迁移
- `/root/ipma/src/resource/drawing.rs` - 布局保存和查询逻辑
- `/root/ipma/src/resource/workstation.rs` - 工位删除时的布局清理
- `/root/ipma/src/init/check.rs` - 表结构检查
- `/root/ipma/web/static/js/modules/visualization/SVGRenderer.js` - 门元素渲染（无需修改）
- `/root/ipma/web/static/js/modules/visualization/SVGDataManager.js` - 布局数据管理（无需修改）
