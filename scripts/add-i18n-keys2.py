#!/usr/bin/env python3
"""Second batch of supplementary i18n keys (login/api/app/utils defaults)."""
import json
from pathlib import Path

I18N_DIR = Path(__file__).resolve().parent.parent / "web" / "static" / "js" / "i18n"

ADDITIONS = {
    "common.delete_confirm": ("确定要删除吗？", "Are you sure you want to delete this?"),
    "common.delete_failed": ("删除失败", "Delete failed"),
    "common.processing": ("处理中...", "Processing..."),
    "common.unknown_error": ("未知错误", "Unknown error"),
    # ---- login ----
    "login.username_password_required": ("请输入用户名和密码", "Please enter username and password"),
    "login.email_code_required": ("请输入邮箱和验证码", "Please enter email and verification code"),
    "login.code_required": ("请输入验证码", "Please enter the verification code"),
    "login.two_factor_failed": ("2FA验证失败", "2FA verification failed"),
    "login.send_failed": ("发送失败", "Send failed"),
    "login.code_sent": ("已发送", "Sent"),
    "login.sending": ("发送中...", "Sending..."),
    "login.network_error": ("请求失败，请检查网络连接", "Request failed. Please check your network connection."),
    "login.network_failed": ("网络连接失败，请检查您的网络设置", "Network connection failed. Please check your network settings."),
    "login.timeout": ("请求超时，请稍后再试", "Request timed out. Please try again later."),
    "login.reset_email_sent": ("密码重置邮件已发送，请查收您的邮箱", "Password reset email sent. Please check your inbox."),
    "login.too_many_attempts": ("登录失败次数过多，请5分钟后再试", "Too many failed login attempts. Please try again in 5 minutes."),
    "login.system_not_init": ("系统未初始化，请联系管理员", "The system is not initialized. Please contact the administrator."),
    "login.email_service_not_configed": ("系统邮件服务未配置，无法发送验证码", "The email service is not configured. Unable to send verification codes."),
    # ---- api (client errors surfaced in toasts) ----
    "api.auth_failed": ("认证失败", "Authentication failed"),
    "api.token_expired": ("登录已过期，请重新登录", "Your session has expired. Please log in again."),
    "api.request_failed": ("API请求失败", "API request failed"),
    "api.parse_failed": ("无法解析响应内容", "Unable to parse the response"),
    "api.network_error": ("API请求失败，请检查网络连接", "API request failed. Please check your network connection."),
    "api.cancelled": ("请求已取消", "Request cancelled"),
    "api.timeout": ("请求超时，请稍后再试", "Request timed out. Please try again later."),
    "api.network_failed": ("网络连接失败，请检查您的网络设置", "Network connection failed. Please check your network settings."),
    # ---- app (bootstrap fatal overlay) ----
    "app.init_failed": ("应用程序初始化失败", "Application initialization failed"),
    "app.load_failed": ("应用程序加载失败", "Application failed to load"),
    "app.refresh_or_contact": ("请刷新页面重试，或联系管理员。", "Please refresh the page or contact the administrator."),
    "app.refresh_page": ("刷新页面", "Refresh Page"),
    # ---- room (extra) ----
    "room.keep_one_network": ("至少需要保留一个网段配置", "At least one network configuration must be kept"),
    "room.select_option": ("选择选项", "Select an option"),
    # ---- workstation (manager already exists) ----
}


def set_nested(d, dotted_key, value):
    parts = dotted_key.split(".")
    cur = d
    for p in parts[:-1]:
        cur = cur.setdefault(p, {})
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
            parts = key.split(".")
            cur = data
            exists = True
            for p in parts[:-1]:
                if p not in cur or not isinstance(cur[p], dict):
                    exists = False
                    break
                cur = cur[p]
            if exists and parts[-1] in cur:
                continue
            set_nested(data, key, pair[idx])
            added += 1
        path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"{fname}: {before} -> {count_nested(data)} (+{added})")


if __name__ == "__main__":
    main()
