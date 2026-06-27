# 用户数据翻译指南

## 文件说明

| 文件 | 用途 |
|------|------|
| `web/static/js/i18n/zh.json` | 系统界面翻译（开发者维护，请勿修改） |
| `web/static/js/i18n/en.json` | 系统界面翻译（开发者维护，请勿修改） |
| `web/static/js/i18n/data_zh.json` | **用户数据翻译（本文档指导维护）** |
| `web/static/js/i18n/data_en.json` | **用户数据翻译（本文档指导维护）** |

系统翻译与数据翻译在加载时自动深合并，数据翻译优先级更高。

---

## 当前内容

`data_zh.json` 和 `data_en.json` 目前包含组织类型名称的翻译：

```json
{
  "organization": {
    "types": {
      "headquarters": "总部",
      "building": "楼号",
      "floor": "楼层",
      "hall": "大厅",
      "office": "办公室",
      "data_center": "机房",
      "workstation": "工位",
      "cabinet": "机柜",
      "cabinet_position": "机位"
    }
  }
}
```

当你在模板管理中手动输入了新的类型名称（如 `server_room`、`meeting_room`），需要在此文件中补充对应翻译。

---

## 编辑步骤

### 1. 确定需要翻译的键

在**模板管理**中查看已创建的模板，记录所有类型名称。例如模板树为：

```
总部 (headquarters)
  └─ 楼号 (building)
       └─ 楼层 (floor)
            ├─ 大厅 (hall)
            └─ 会议室 (meeting_room)  ← 新增的自定义类型
```

其中 `meeting_room` 是手动输入的，需要补充翻译。

### 2. 编辑中文翻译

打开 `web/static/js/i18n/data_zh.json`，在 `types` 对象中添加：

```json
{
  "organization": {
    "types": {
      "headquarters": "总部",
      "building": "楼号",
      "floor": "楼层",
      "hall": "大厅",
      "office": "办公室",
      "data_center": "机房",
      "workstation": "工位",
      "cabinet": "机柜",
      "cabinet_position": "机位",
      "meeting_room": "会议室"
    }
  }
}
```

### 3. 编辑英文翻译

打开 `web/static/js/i18n/data_en.json`，添加对应的英文翻译：

```json
{
  "organization": {
    "types": {
      "headquarters": "Headquarters",
      "building": "Building",
      "floor": "Floor",
      "hall": "Hall",
      "office": "Office",
      "data_center": "Data Center",
      "workstation": "Workstation",
      "cabinet": "Cabinet",
      "cabinet_position": "Cabinet Position",
      "meeting_room": "Meeting Room"
    }
  }
}
```

### 4. 刷新页面

保存文件后刷新浏览器即可生效，无需重启服务。

---

## 规则

- **键名**必须与模板中输入的类型名称完全一致（区分大小写）
- **键名**只能包含字母、数字、下划线（如 `server_room`），不能包含空格或特殊字符
- 如果某个类型没有对应翻译，系统会直接显示类型名称原文
- 两个文件（`data_zh.json` 和 `data_en.json`）的键应当保持一致
- JSON 格式必须合法：最后一个键后面不能有逗号

---

## 验证

修改后可在**模板管理**列表中查看效果：

- 模板层级路径应显示翻译后的名称（如 `总部 → 楼号 → 楼层 → 会议室`）
- 新增节点时，类型下拉选项应显示翻译后的名称
- 如果显示为英文键名（如 `meeting_room`），说明翻译未生效，请检查拼写和 JSON 格式

---

## 备忘

### 后续新增 create 模块在线修改翻译

当前阶段用户需手动编辑 `data_zh.json` / `data_en.json` 文件来维护数据翻译。

后续计划开发独立的 `create` 模块，提供在线界面来管理用户数据翻译：

- 在网页中直接新增、编辑、删除翻译条目
- 自动同步到 `data_zh.json` 和 `data_en.json`
- 支持批量导入/导出
- 无需手动编辑 JSON 文件

该模块上线后，本文档的编辑步骤将被在线操作替代。
