#!/usr/bin/env python3
"""Add missing i18n keys to both zh.json and en.json, keeping them in parity.

Only inserts keys that do not already exist (never overwrites). Run from the
repo root or pass the web static dir. Idempotent.
"""
import json
import sys
from pathlib import Path

I18N_DIR = Path(__file__).resolve().parent.parent / "web" / "static" / "js" / "i18n"

# { key_path: (zh, en) }  — dot notation for nested keys
ADDITIONS = {
    # ---- common ----
    "common.yes": ("是", "Yes"),
    "common.no": ("否", "No"),
    "common.unknown": ("未知", "Unknown"),
    "common.save_success": ("保存成功", "Saved successfully"),
    "common.save_failed": ("保存失败", "Save failed"),
    "common.delete_success": ("删除成功", "Deleted successfully"),
    "common.operation_failed_retry": ("操作失败，请重试", "Operation failed, please retry"),
    "common.missing_id_param": ("操作失败：缺少ID参数", "Operation failed: missing ID parameter"),
    "common.check_input": ("操作失败，请检查输入信息", "Operation failed, please check your input"),
    "common.refresh_failed": ("刷新失败，请重试", "Refresh failed, please retry"),
    "common.refreshing": ("刷新中...", "Refreshing..."),
    "common.records": ("条", "records"),
    "common.password_placeholder": ("请输入密码", "Please enter password"),
    # ---- cabinet ----
    "cabinet.no_positions_hint": ("暂无机位，点击下方按钮添加", "No positions yet. Click the button below to add one."),
    # ---- cabinet_position ----
    "cabinet_position.load_failed": ("获取机位数据失败", "Failed to load cabinet position data"),
    "cabinet_position.save_success": ("机位保存成功", "Cabinet position saved successfully"),
    "cabinet_position.name_required": ("名称不能为空", "Name cannot be empty"),
    "cabinet_position.cabinet_required": ("请选择机柜", "Please select a cabinet"),
    "cabinet_position.start_u_invalid": ("起始U位必须是有效的正数", "Start U must be a valid positive number"),
    "cabinet_position.end_u_invalid": ("结束U位必须是有效的正数", "End U must be a valid positive number"),
    "cabinet_position.start_gt_end": ("起始U位不能大于结束U位", "Start U cannot be greater than end U"),
    "cabinet_position.edit": ("编辑机位", "Edit Position"),
    "cabinet_position.add": ("添加机位", "Add Position"),
    # ---- workstation ----
    "workstation.load_failed": ("获取工位数据失败", "Failed to load workstation data"),
    "workstation.save_success": ("工位保存成功", "Workstation saved successfully"),
    "workstation.name_required": ("工位名称不能为空", "Workstation name cannot be empty"),
    "workstation.room_required": ("请选择房间", "Please select a room"),
    "workstation.edit": ("编辑工位", "Edit Workstation"),
    "workstation.add": ("添加工位", "Add Workstation"),
    # ---- room ----
    "room.load_failed": ("加载房间数据失败", "Failed to load room data"),
    "room.fetch_failed": ("获取房间数据失败", "Failed to fetch room data"),
    "room.save_failed": ("房间保存失败", "Failed to save room"),
    "room.save_success": ("房间保存成功", "Room saved successfully"),
    "room.no_id_returned": ("房间创建成功但未返回ID", "Room was created but no ID was returned"),
    "room.edit": ("编辑房间", "Edit Room"),
    "room.add": ("添加房间", "Add Room"),
    # ---- ip ----
    "ip.select_device": ("-- 选择设备 --", "-- Select Device --"),
    "ip.no_device_data": ("暂无设备数据", "No devices"),
    "ip.no_snmp_device": ("暂无配置SNMP的设备", "No devices with SNMP configured"),
    "ip.select_network": ("-- 选择网段 --", "-- Select Network --"),
    "ip.no_network_data": ("暂无网段数据", "No networks"),
    "ip.select_device_first": ("请先选择一个设备", "Please select a device first"),
    "ip.select_network_first": ("请先选择一个网段", "Please select a network first"),
    "ip.pulling": ("拉取中...", "Pulling..."),
    "ip.pull_mac_success": ("MAC数据拉取成功", "MAC data pulled successfully"),
    "ip.pull_mac_failed": ("MAC数据拉取失败", "Failed to pull MAC data"),
    "ip.no_ip_data": ("暂无IP数据", "No IP data"),
    "ip.server_connection_failed": ("服务器连接失败，请检查网络或联系管理员", "Server connection failed. Please check your network or contact the administrator."),
    "ip.load_failed_short": ("加载失败", "Load failed"),
    # ---- network ----
    "network.no_region_data": ("暂无网络区域数据", "No network regions"),
    "network.no_match_data": ("没有找到匹配的网段数据", "No matching network data found"),
    "network.fetch_failed": ("获取网络数据失败", "Failed to fetch network data"),
    "network.fetch_region_failed": ("获取网络区域数据失败", "Failed to fetch network region data"),
    "network.ipv4_usage_updated": ("IPv4使用情况已更新", "IPv4 usage updated"),
    "network.ipv6_usage_updated": ("IPv6使用情况已更新", "IPv6 usage updated"),
    "network.cidr_ipv4_example": ("例如: 10.0.0.0/8", "e.g. 10.0.0.0/8"),
    "network.cidr_ipv6_example": ("例如: 2001:db8::/32", "e.g. 2001:db8::/32"),
    "network.region_name_required": ("网络区域名称不能为空", "Network region name cannot be empty"),
    "network.name_required": ("网络名称不能为空", "Network name cannot be empty"),
    "network.region_required": ("请选择网络区域", "Please select a network region"),
    "network.cidr_required": ("至少需要提供一个有效的IPv4或IPv6 CIDR", "At least one valid IPv4 or IPv6 CIDR is required"),
    "network.dns_limit": ("DNS服务器数量不能超过5个", "No more than 5 DNS servers are allowed"),
    "network.edit_region": ("编辑网络区域", "Edit Network Region"),
    "network.edit_network": ("编辑网络", "Edit Network"),
    # ---- user ----
    "user.password_mismatch": ("两次输入的密码不一致", "The passwords entered do not match"),
    "user.update_success": ("用户更新成功", "User updated successfully"),
    "user.add_success": ("用户添加成功", "User added successfully"),
    "user.update_failed": ("更新用户失败", "Failed to update user"),
    "user.add_failed": ("添加用户失败", "Failed to add user"),
    "user.save_failed_network": ("保存用户失败，请检查网络连接", "Failed to save user. Please check your network connection."),
    # ---- two_factor ----
    "two_factor.disable_confirm": ("确定要禁用双因素认证吗？", "Are you sure you want to disable two-factor authentication?"),
    "two_factor.open_modal_failed": ("无法打开2FA模态框", "Unable to open 2FA modal"),
    "two_factor.modal_load_failed": ("2FA模态框加载失败", "Failed to load 2FA modal"),
    "two_factor.admin_only": ("权限不足，只有管理员可以为其他用户操作2FA", "Insufficient permissions. Only an administrator can manage 2FA for other users."),
    "two_factor.fetch_config_failed": ("获取2FA配置失败", "Failed to fetch 2FA configuration"),
    "two_factor.fetch_config_network": ("获取2FA配置失败，请检查网络连接", "Failed to fetch 2FA configuration. Please check your network connection."),
    "two_factor.enable_success": ("双因素认证已成功启用！", "Two-factor authentication enabled successfully!"),
    "two_factor.enable_failed": ("启用失败", "Enable failed"),
    "two_factor.enable_failed_network": ("启用失败，请检查网络连接", "Enable failed. Please check your network connection."),
    "two_factor.disable_success": ("双因素认证已成功禁用", "Two-factor authentication disabled successfully"),
    "two_factor.disable_failed": ("禁用失败", "Disable failed"),
    "two_factor.disable_failed_network": ("禁用失败，请检查网络连接", "Disable failed. Please check your network connection."),
    # ---- notifications (the bell/alerts) ----
    "notifications.no_data": ("暂无通知数据", "No notifications"),
    "notifications.mark_read": ("标记已读", "Mark as read"),
    "notifications.cleared_read": ("已读通知已清除", "Read notifications cleared"),
    "notifications.confirm_clear_read": ("确定要清除所有已读通知吗？", "Clear all read notifications?"),
    # ---- notification (MAC-change email config) ----
    "notification.invalid_email": ("请输入有效的邮箱地址", "Please enter a valid email address"),
    "notification.smtp_not_configed": ("请先在系统设置中配置SMTP服务器", "Please configure the SMTP server in system settings first"),
    "notification.email_saved": ("MAC变动通知邮箱已保存", "MAC-change notification emails saved"),
    "notification.email_cleared": ("MAC变动通知邮箱已清除", "MAC-change notification emails cleared"),
    "notification.check_smtp_failed": ("检查SMTP配置失败，请重试", "Failed to check SMTP configuration. Please retry."),
    "notification.save_success": ("通知设置保存成功", "Notification settings saved successfully"),
    "notification.save_failed": ("通知设置保存失败", "Failed to save notification settings"),
    "notification.save_error": ("保存通知设置失败", "Failed to save notification settings"),
    # ---- system ----
    "system.config_save_success": ("系统配置保存成功", "System configuration saved successfully"),
    "system.config_save_failed": ("系统配置保存失败", "Failed to save system configuration"),
    "system.config_saved_restart_needed": ("配置已更新，需要重启应用系统以使配置生效", "Configuration updated. The application must be restarted for it to take effect."),
    # ---- smtp ----
    "smtp.server_settings": ("SMTP服务器设置", "SMTP Server Settings"),
    "smtp.save_success": ("SMTP配置保存成功", "SMTP configuration saved successfully"),
    "smtp.save_failed": ("SMTP配置保存失败", "Failed to save SMTP configuration"),
    "smtp.test_success": ("SMTP连接测试成功", "SMTP connection test succeeded"),
    "smtp.test_failed": ("SMTP连接测试失败", "SMTP connection test failed"),
    "smtp.test_error": ("测试SMTP连接失败", "Failed to test SMTP connection"),
    "smtp.password_configured": ("已配置（留空保持不变）", "Configured (leave blank to keep unchanged)"),
    "smtp.password_input": ("请输入密码", "Please enter the password"),
    "smtp.username_placeholder": ("请输入SMTP用户名", "Enter SMTP username"),
    "smtp.password_placeholder": ("请输入SMTP密码", "Enter SMTP password"),
    "smtp.from_placeholder": ("请输入发件人邮箱", "Enter sender email"),
    # ---- config ----
    "config.session_settings": ("会话设置", "Session Settings"),
    # ---- logs ----
    "logs.view_detail_failed": ("查看详情失败", "Failed to view details"),
    "logs.detail_title": ("操作日志详情", "Operation Log Details"),
    "logs.no_detail": ("无详细信息", "No details"),
    "logs.earliest_label": ("最早", "Earliest"),
    # ---- import_export ----
    "import_export.download_template_failed": ("下载模板失败", "Failed to download template"),
    "import_export.importing": ("正在导入数据，请稍候...", "Importing data, please wait..."),
    "import_export.import_success": ("数据导入完成", "Data import complete"),
    "import_export.import_failed": ("数据导入失败", "Data import failed"),
    "import_export.export_csv_failed": ("导出CSV数据失败", "Failed to export CSV data"),
    "import_export.export_db_failed": ("导出数据库失败", "Failed to export database"),
    "import_export.export_db_success": ("数据库导出成功", "Database exported successfully"),
    "import_export.backup_failed": ("备份配置失败", "Failed to back up configuration"),
    "import_export.restore_success": ("配置恢复成功", "Configuration restored successfully"),
    "import_export.restore_failed": ("配置恢复失败", "Failed to restore configuration"),
    "import_export.restore_error": ("恢复配置失败", "Failed to restore configuration"),
    # ---- dashboard ----
    "dashboard.no_network_data": ("暂无网络数据", "No network data"),
    "dashboard.no_ip_data": ("暂无IP数据", "No IP data"),
    "dashboard.no_room_data": ("暂无房间数据", "No room data"),
    "dashboard.no_cabinet_data": ("暂无机柜数据", "No cabinet data"),
    "dashboard.no_log_data": ("暂无活动记录", "No recent activity"),
    "dashboard.unit_workstation": ("工位", "workstations"),
    "dashboard.unit_position": ("机位", "positions"),
    # ---- device (supplement) ----
    "device.cabinet_position": ("机位", "Cabinet Position"),
    "device.switch": ("交换机", "Switch"),
    "device.ip_address": ("IP地址", "IP Address"),
    "device.mac_address": ("MAC地址", "MAC Address"),
    "device.ip_list": ("IP地址列表", "IP Address List"),
    "device.delete_network_card": ("删除此网卡", "Remove this network card"),
    "device.delete_network_port": ("删除此网口", "Remove this network port"),
    "device.no_ports": ("暂无端口数据", "No port data"),
    "device.no_snmp_config": ("该设备未配置SNMP信息，无法获取端口数据", "This device has no SNMP configuration, unable to fetch port data"),
    "device.load_ports_failed": ("加载端口数据失败", "Failed to load port data"),
    "device.port_save_refresh_failed": ("端口保存成功，但刷新数据失败，请手动刷新", "Port saved successfully, but data refresh failed. Please refresh manually."),
    "device.port_submit_failed": ("提交端口表单失败", "Failed to submit port form"),
    "device.port_sync_success": ("端口同步成功", "Ports synced successfully"),
    "device.sync_ports_from_snmp": ("从SNMP获取端口", "Fetch ports from SNMP"),
    "device.search_ip_mac": ("搜索IP或MAC地址...", "Search IP or MAC address..."),
    "device.loading_mac_table": ("正在加载MAC表...", "Loading MAC table..."),
    "device.load_mac_failed": ("加载MAC表失败", "Failed to load MAC table"),
    "device.syncing_mac": ("正在从SNMP同步MAC表...", "Syncing MAC table from SNMP..."),
    "device.sync_from_snmp": ("从SNMP同步", "Sync from SNMP"),
    "device.unknown_network": ("未知网段", "Unknown network"),
    "device.loading_lldp": ("正在加载LLDP邻居信息...", "Loading LLDP neighbors..."),
    "device.load_lldp_failed": ("加载LLDP邻居失败", "Failed to load LLDP neighbors"),
    "device.syncing_lldp": ("正在从SNMP同步LLDP信息...", "Syncing LLDP info from SNMP..."),
    "device.local_port": ("本地端口", "Local Port"),
    "device.neighbor_device": ("邻居设备", "Neighbor Device"),
    "device.neighbor_port": ("邻居端口", "Neighbor Port"),
    "device.system_description": ("系统描述", "System Description"),
    "device.snmp_test_success": ("SNMP连接测试成功", "SNMP connection test succeeded"),
    "device.snmp_test_failed": ("SNMP连接测试失败", "SNMP connection test failed"),
    "device.save_before_snmp": ("请先保存设备后再获取SNMP信息", "Please save the device before fetching SNMP information"),
    "device.snmp_info_success": ("设备信息获取成功", "Device information fetched successfully"),
    "device.snmp_info_failed": ("获取设备信息失败", "Failed to fetch device information"),
    "device.select_device": ("-- 选择设备 --", "-- Select Device --"),
    # ---- ip (delete aria) ----
    "ip.delete_ip": ("删除此IP", "Remove this IP"),
    # ---- net_outlet (peer section) ----
    "net_outlet.peer_type": ("对端类型", "Peer Type"),
    "net_outlet.peer_type_none": ("无", "None"),
    "net_outlet.peer_type_outlet": ("信息点", "Outlet"),
    "net_outlet.peer_room": ("对端房间", "Peer Room"),
    "net_outlet.select_peer_room": ("选择对端房间", "Select Peer Room"),
    "net_outlet.peer_outlet": ("对端信息点", "Peer Outlet"),
    "net_outlet.select_peer_outlet": ("选择对端信息点", "Select Peer Outlet"),
    # ---- org_template ----
    "org_template.select_icon": ("选择图标", "Select Icon"),
    # ---- common.testing ----
    "common.testing": ("测试中...", "Testing..."),
}


def set_nested(d, dotted_key, value):
    parts = dotted_key.split(".")
    cur = d
    for p in parts[:-1]:
        if p not in cur or not isinstance(cur[p], dict):
            cur[p] = {}
        cur = cur[p]
    cur[parts[-1]] = value


def count_nested(d):
    return sum(count_nested(v) if isinstance(v, dict) else 1 for v in d.values())


def main():
    for fname, idx in (("zh.json", 0), ("en.json", 1)):
        path = I18N_DIR / fname
        data = json.loads(path.read_text(encoding="utf-8"))
        before = count_nested(data)
        added = 0
        for key, pair in ADDITIONS.items():
            # only set if the leaf is missing
            parts = key.split(".")
            cur = data
            exists = True
            for p in parts[:-1]:
                if p not in cur or not isinstance(cur[p], dict):
                    exists = False
                    break
                cur = cur[p]
            if exists and parts[-1] in cur:
                continue  # never overwrite
            set_nested(data, key, pair[idx])
            added += 1
        path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"{fname}: {before} -> {count_nested(data)} keys (+{added})")


if __name__ == "__main__":
    sys.exit(main())
