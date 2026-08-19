#!/usr/bin/env python3
"""本轮改动的 i18n 键批量更新：房型/信息点唯一/2FA/关于卡片/通知配置/组织图标等。"""
import json
import sys

BASE = "/media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/i18n"

# (路径, 键, zh, en)；路径为顶层键的点分路径
NEW_KEYS = [
    # 房间类型
    ("room", "type_lobby", "大厅", "Lobby"),
    ("room", "type_reception", "前台", "Reception"),
    ("room", "type_other", "其他", "Other"),
    # 设备模态框筛选
    ("device", "room_type_filter", "房间类型", "Room Type"),
    ("device", "all_room_types", "全部房间类型", "All Room Types"),
    # 信息点唯一性（前端本地校验）
    ("net_outlet", "name_required", "请填写信息点名称", "Outlet name is required"),
    ("net_outlet", "name_duplicate", "信息点名称「{{name}}」重复（信息点名称在所有房间内唯一）",
     'Outlet name "{{name}}" is duplicated (outlet names must be unique across all rooms)'),
    # 可视化筛选
    ("viz", "all_room_types", "全部类型", "All Types"),
    # 关于卡片
    ("system", "about", "关于", "About"),
    ("system", "project_url", "项目地址", "Project URL"),
    ("system", "author", "作者", "Author"),
    ("system", "contact", "联系方式", "Contact"),
    ("system", "open_source_components", "开源组件", "Open Source Components"),
    ("system", "open_source_intro",
     "本项目以 GPL-3.0-or-later 授权发布，使用了以下开源组件（完整清单见 NOTICE 文件）。",
     "This project is licensed under GPL-3.0-or-later and uses the following open source components (see the NOTICE file for details)."),
    # 组织模板图标分组与含义
    ("org_template", "icon_group_business", "业务组织", "Business Organization"),
    ("org_template", "icon_group_location", "物理地点", "Physical Locations"),
    ("org_template", "icon_group_space", "功能空间", "Functional Spaces"),
    ("org_template", "icon_corp", "集团/总公司", "Group / Headquarters"),
    ("org_template", "icon_division", "事业部/板块", "Division / Segment"),
    ("org_template", "icon_subsidiary", "子公司/分公司", "Subsidiary / Branch"),
    ("org_template", "icon_center", "中心/大部门", "Center / Major Dept."),
    ("org_template", "icon_department", "部门", "Department"),
    ("org_template", "icon_team", "科室/小组", "Team"),
    ("org_template", "icon_person", "岗位/人员", "Position / Person"),
    ("org_template", "icon_region", "区域（大区）", "Region"),
    ("org_template", "icon_city", "城市", "City"),
    ("org_template", "icon_campus", "园区/基地", "Campus / Base"),
    ("org_template", "icon_building", "楼宇", "Building"),
    ("org_template", "icon_floor", "楼层", "Floor"),
    ("org_template", "icon_zone", "区域分区", "Zone"),
    ("org_template", "icon_hall", "大厅", "Hall"),
    ("org_template", "icon_reception", "前台", "Reception"),
    ("org_template", "icon_office", "办公室", "Office"),
    ("org_template", "icon_data_center", "机房", "Data Center"),
    ("org_template", "icon_telecom_closet", "弱电井", "Telecom Closet"),
    ("org_template", "icon_workstation", "工位", "Workstation"),
    ("org_template", "icon_cabinet", "机柜", "Cabinet"),
    ("org_template", "icon_cabinet_position", "机位", "Cabinet Position"),
    ("org_template", "icon_org", "通用节点", "Generic Node"),
    # 2FA 二维码生成失败
    ("server.auth", "qr_generate_failed", "登录二次验证二维码生成失败：{{error}}",
     "Failed to generate 2FA QR code: {{error}}"),
    # 组织模板：被引用类型路径被破坏
    ("server.org_template", "in_use_path_broken",
     "类型「{{name}}」正被 {{count}} 个组织节点使用，无法删除、重命名或调整其在层级中的位置",
     "Type \"{{name}}\" is used by {{count}} organization node(s) and cannot be deleted, renamed, or repositioned in the hierarchy"),
]

# 服务端消息文案更新
TEXT_UPDATES = {
    ("server", "net_outlet", "name_exists"): (
        "信息点名称已存在（信息点名称在所有房间内唯一）",
        "Outlet name already exists (outlet names must be unique across all rooms)"),
    ("server", "room", "validation", "type_invalid"): (
        "房间类型必须是 office、lobby、reception、data_center、telecom_closet 或 other",
        "Room type must be one of: office, lobby, reception, data_center, telecom_closet, other"),
}

# 顶层键更名：(旧路径, 新键)；旧键移除
RENAMES = [
    (("system", "smtp"), "notification"),  # SMTP配置 → 通知配置
]

REMOVALS = [
    ("server", "org_template", "in_use_structure_changed"),
]


def get_section(data, path):
    section = data
    for key in path:
        section = section.setdefault(key, {})
    return section


def main():
    for lang in ("zh", "en"):
        path = f"{BASE}/{lang}.json"
        with open(path, encoding="utf-8") as f:
            data = json.load(f)

        for path_keys, key, zh_text, en_text in NEW_KEYS:
            section = get_section(data, path_keys.split("."))
            section[key] = zh_text if lang == "zh" else en_text

        for path_keys, (zh_text, en_text) in TEXT_UPDATES.items():
            section = data
            for key in path_keys[:-1]:
                section = section[key]
            section[path_keys[-1]] = zh_text if lang == "zh" else en_text

        for old_path, new_key in RENAMES:
            section = data
            for key in old_path[:-1]:
                section = section[key]
            old_key = old_path[-1]
            if old_key in section:
                value = section.pop(old_key)
                if lang == "zh":
                    value = "通知配置"
                else:
                    value = "Notification"
                # 保持原位置：重建该 section 以新键放回原顺序位置
                items = list(section.items())
                rebuilt = {}
                # 找到被移除键原本的邻居（简单追加即可，键顺序对 UI 无影响）
                rebuilt.update(section)
                rebuilt[new_key] = value
                section.clear()
                section.update(rebuilt)

        for path_keys in REMOVALS:
            section = data
            for key in path_keys[:-1]:
                section = section[key]
            section.pop(path_keys[-1], None)

        with open(path, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=2)
            f.write("\n")
        print(f"{lang}.json updated")


if __name__ == "__main__":
    sys.exit(main())
