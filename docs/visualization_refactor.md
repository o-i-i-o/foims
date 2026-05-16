# IPMA 可视化模块重构 — 功能逻辑文档

> 版本：0.8.75  
> 更新日期：2026-05-16

---

## 一、重构概述

本次重构对可视化模块进行全面改造，核心变更包括：

1. 机柜可视化从按网络区域筛选改为按房间筛选，遵循"房间-机柜-机位"层级关系
2. 机位位置信息从 positions 表的 start_u/end_u 字段获取
3. 工位可视化基于 workstation_layouts 表，机柜可视化基于 cabinet_layouts 表
4. 工位可视化支持"door"标志元素的识别与展示
5. 优化数据查询，新增聚合API一次返回机柜+机位+布局数据

---

## 二、数据模型关系

### 2.1 核心实体关系图

```
rooms ──1:N── cabinets ──1:N── positions (start_u, end_u)
  │                │              │
  │                │              └── device_type = 'cabinet_position' / 'switch'
  │                │
  │                └── cabinet_layouts (x, y, width, height, rotation)
  │
  └── workstations ── workstation_layouts (x, y, width, height, rotation)
       │                    │
       │                    └── door 元素 (固定ID, 非数据库实体)
       │
       └── ips (工位IP信息)
```

### 2.2 布局表结构

**workstation_layouts 表：**

| 列名 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | UUID | PK | 主键 |
| workstation_id | UUID | UNIQUE, FK → workstations(id) ON DELETE CASCADE | 工位ID |
| x | INTEGER | NOT NULL DEFAULT 0 | X坐标 |
| y | INTEGER | NOT NULL DEFAULT 0 | Y坐标 |
| width | INTEGER | NOT NULL DEFAULT 160 | 宽度 |
| height | INTEGER | NOT NULL DEFAULT 160 | 高度 |
| rotation | INTEGER | NOT NULL DEFAULT 0 | 旋转角度 |
| created_at | TIMESTAMPTZ | NOT NULL | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL | 更新时间 |

**cabinet_layouts 表：**

| 列名 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | UUID | PK | 主键 |
| cabinet_id | UUID | UNIQUE, FK → cabinets(id) ON DELETE CASCADE | 机柜ID |
| x | INTEGER | NOT NULL DEFAULT 0 | X坐标 |
| y | INTEGER | NOT NULL DEFAULT 0 | Y坐标 |
| width | INTEGER | NOT NULL DEFAULT 160 | 宽度 |
| height | INTEGER | NOT NULL DEFAULT 160 | 高度 |
| rotation | INTEGER | NOT NULL DEFAULT 0 | 旋转角度 |
| created_at | TIMESTAMPTZ | NOT NULL | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL | 更新时间 |

### 2.3 positions 表关键字段

| 列名 | 类型 | 说明 |
|------|------|------|
| id | UUID | 主键 |
| name | VARCHAR(50) | 机位名称 |
| cabinet_id | UUID | 关联机柜 |
| start_u | INTEGER | 起始U位（可视化定位依据） |
| end_u | INTEGER | 结束U位（可视化定位依据） |
| device_type | VARCHAR(20) | 'cabinet_position' 或 'switch' |
| device_id | UUID | 关联设备ID |

---

## 三、核心业务逻辑

### 3.1 工位可视化流程

```
用户选择房间 → room-select 下拉框变更
    │
    ▼
autoDrawWorkstations(roomId)
    │
    ├── 1. 加载已保存布局
    │   GET /api/resources/layouts/workstation/{roomId}
    │   返回: [{id, workstation_id, x, y, width, height, rotation, element_type}]
    │
    ├── 2. 获取房间工位数据
    │   GET /api/resources/workstations?room_id={roomId}
    │
    ├── 3. 获取IP数据（用于显示工位IP信息）
    │   GET /api/resources/ip
    │
    ├── 4. 绘制门(door)元素
    │   renderer.drawDoor() → 固定ID "00000000-0000-0000-0000-000000000001"
    │
    ├── 5. 恢复已保存布局位置 / 自动排列
    │   有布局数据: 恢复每个工位和门的位置
    │   无布局数据: 自动网格排列
    │
    └── 6. 调整 viewBox 适配所有元素
```

### 3.2 机柜可视化流程（重构后）

```
用户选择房间 → cabinet-room-select 下拉框变更
    │
    ▼
autoDrawCabinetPositions(roomId)
    │
    ├── 1. 获取房间机柜+机位+布局数据（一次请求）
    │   GET /api/resources/layouts/room-cabinets/{roomId}
    │   返回: [{id, name, capacity, positions: [...], layout: {...}}]
    │
    ├── 2. 加载已保存布局
    │   GET /api/resources/layouts/positions/{roomId}
    │   返回: [{id, cabinet_id, x, y, width, height, rotation, element_type}]
    │
    ├── 3. 恢复已保存布局位置 / 自动排列
    │   有布局数据: 恢复每个机柜位置
    │   无布局数据: 从底部向上自动排列
    │
    ├── 4. 绘制每个机柜
    │   renderer.drawCabinet(cabinet) → rect + U位刻度 + 名称
    │
    ├── 5. 绘制机柜内机位
    │   renderer.drawCabinetPosition(position, cabinet)
    │   使用 position.start_u / end_u 计算Y坐标
    │
    └── 6. 调整 viewBox 适配所有元素
```

### 3.3 机位U位定位算法

```javascript
// 机柜参数
const U_HEIGHT = 20;  // 每U高度20px
const HEADER_HEIGHT = 40;  // 机柜头部高度

// 计算机位Y坐标
const positionY = cabinetBaseY + HEADER_HEIGHT + (cabinetCapacity - position.end_u) * U_HEIGHT;
const positionHeight = (position.end_u - position.start_u + 1) * U_HEIGHT;
```

---

## 四、"door"标志元素处理逻辑

### 4.1 概述

door 元素是工位可视化中的特殊标志，表示房间入口参考点。它不是数据库实体，而是前端虚拟元素。

### 4.2 door 元素属性

| 属性 | 值 | 说明 |
|------|------|------|
| 固定ID | `00000000-0000-0000-0000-000000000001` | 用于布局保存/恢复 |
| CSS类 | `door-element` | 用于样式和拖拽识别 |
| element_type | `door` | 保存布局时的类型标识 |
| 默认位置 | x=50, y=100, width=40, height=80 | 首次绘制时 |
| tooltip | "房间入口参考点" | 悬停提示 |

### 4.3 door 元素SVG结构

```svg
<g class="door-element" data-id="00000000-0000-0000-0000-000000000001">
  <rect class="door-frame" x="50" y="100" width="40" height="80" 
        fill="#e0e0e0" stroke="#999" stroke-width="2" rx="3"/>
  <circle class="door-handle" cx="80" cy="140" r="5" fill="#666"/>
  <text class="door-label" x="70" y="145" text-anchor="middle" fill="#333">门</text>
</g>
```

### 4.4 door 保存与恢复

**保存时：**
```javascript
// SVGDataManager.saveLayout()
const elementType = el.classList.contains("door-element") ? "door" : "workstation";
layoutItems.push({ id: el.dataset.id, position: {...}, element_type: elementType });
```

**恢复时：**
```javascript
// SVGDataManager.loadSavedLayout()
const doorItem = layoutData.find(item => 
    item.element_type === "door" || 
    item.id === "00000000-0000-0000-0000-000000000001"
);
if (doorItem && doorItem.position) {
    // 恢复门框rect、门把手circle、标签text的位置
}
```

### 4.5 door 拖拽处理

```javascript
// SVGCore.js 拖拽逻辑
if (element.classList.contains("door-element")) {
    const doorHandle = element.querySelector("circle");
    if (doorHandle) {
        // 更新门把手位置（相对于门框）
        doorHandle.setAttribute("cx", x + relCx);
        doorHandle.setAttribute("cy", y + relCy);
    }
}
```

---

## 五、API接口设计

### 5.1 布局管理接口

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /api/resources/layouts | 保存布局（工位/机柜） |
| GET | /api/resources/layouts/workstation/{room_id} | 获取房间工位布局 |
| DELETE | /api/resources/layouts/workstation/{room_id} | 删除房间工位布局 |
| GET | /api/resources/layouts/positions/{room_id} | 获取房间机柜布局 |
| DELETE | /api/resources/layouts/positions/{room_id} | 删除房间机柜布局 |
| **GET** | **/api/resources/layouts/room-cabinets/{room_id}** | **获取房间机柜+机位+布局（新增）** |

### 5.2 保存布局请求体

```json
{
  "type": "workstation",
  "room_id": "uuid",
  "network_region_id": null,
  "cabinet_id": null,
  "layout": [
    {
      "id": "workstation-uuid",
      "position": {"x": 150, "y": 100},
      "element_type": "workstation"
    },
    {
      "id": "00000000-0000-0000-0000-000000000001",
      "position": {"x": 50, "y": 100},
      "element_type": "door"
    }
  ]
}
```

```json
{
  "type": "cabinet",
  "room_id": "uuid",
  "network_region_id": null,
  "cabinet_id": null,
  "layout": [
    {
      "id": "cabinet-uuid",
      "position": {"x": 50, "y": 500},
      "element_type": "cabinet"
    }
  ]
}
```

### 5.3 获取房间机柜数据响应

`GET /api/resources/layouts/room-cabinets/{room_id}`

```json
{
  "success": true,
  "data": [
    {
      "id": "cabinet-uuid",
      "name": "1#",
      "room_id": "room-uuid",
      "capacity": 42,
      "description": null,
      "positions": [
        {
          "id": "position-uuid",
          "name": "服务器A",
          "cabinet_id": "cabinet-uuid",
          "start_u": 1,
          "end_u": 4,
          "description": null,
          "device_type": "cabinet_position",
          "device_id": null
        }
      ],
      "layout": {
        "x": 50,
        "y": 500,
        "width": 160,
        "height": 920,
        "rotation": 0
      }
    }
  ]
}
```

---

## 六、前端架构

### 6.1 模块职责

| 模块 | 文件 | 职责 |
|------|------|------|
| SVGCore | SVGCore.js | SVG初始化、事件系统、拖拽、对齐线、tooltip |
| SVGRenderer | SVGRenderer.js | SVG元素渲染（工位/机柜/机位/门） |
| SVGDataManager | SVGDataManager.js | 数据获取、布局加载/保存/删除 |
| SVGVisualization | SVGVisualization.js | 可视化入口，组合Core/Renderer/DataManager |
| visualizationManager | visualizationManager.js | 页面级管理、Tab切换、事件绑定 |

### 6.2 数据流向

```
用户操作 → visualizationManager
    │
    ├── 工位可视化 → SVGVisualization(type="workstation")
    │   ├── SVGDataManager.fetchWorkstationsByRoom(roomId)
    │   ├── SVGDataManager.loadSavedLayout(roomId)
    │   ├── SVGRenderer.drawWorkstation(workstation)
    │   └── SVGRenderer.drawDoor()
    │
    └── 机柜可视化 → SVGVisualization(type="cabinet")
        ├── SVGDataManager.fetchCabinetsByRoom(roomId)
        ├── SVGDataManager.loadSavedLayout(roomId)
        ├── SVGRenderer.drawCabinet(cabinet)
        └── SVGRenderer.drawCabinetPosition(position, cabinet)
```

### 6.3 房间筛选机制

| 可视化类型 | 筛选下拉框 | 数据源 |
|-----------|-----------|--------|
| 工位可视化 | `room-select` | `loadRoomsForSelect()` |
| 机柜可视化 | `cabinet-room-select` | `loadDataCenterRoomsForSelect()` |

---

## 七、关键业务规则

| 规则 | 说明 |
|------|------|
| 层级关系 | 机柜可视化遵循"房间→机柜→机位"层级 |
| U位定位 | 机位位置由 positions.start_u/end_u 决定，非手动定位 |
| 布局持久化 | 工位布局存 workstation_layouts，机柜布局存 cabinet_layouts |
| door虚拟元素 | 门不是数据库实体，使用固定UUID，布局保存时标记为 element_type="door" |
| 级联删除 | 删除工位/机柜时自动删除对应布局记录（ON DELETE CASCADE） |
| 按房间筛选 | 两种可视化均按房间维度筛选数据 |
| 性能优化 | 机柜可视化使用聚合API一次获取机柜+机位+布局数据 |

---

## 八、文件修改清单

### 后端文件

| 文件 | 修改内容 |
|------|----------|
| src/resource/drawing.rs | get_positions_layout 完整实现；delete_positions_layout 完整实现；新增 get_room_cabinets_with_positions |
| src/routes/mod.rs | 路由参数从 network_region_id 改为 room_id；新增 room-cabinets 路由 |

### 前端文件

| 文件 | 修改内容 |
|------|----------|
| SVGDataManager.js | fetchCabinetsByRoom 替代 fetchCabinetsByNetworkRegion；drawCabinetPositionsWithIp 使用内嵌数据 |
| SVGVisualization.js | autoDrawCabinetPositions 参数改为 roomId |
| SVGCore.js | 移除 currentNetworkRegionId 属性 |
| visualizationManager.js | 机柜可视化使用 cabinet-room-select；加载数据中心房间列表 |
| main.html | network-region-select → cabinet-room-select |
