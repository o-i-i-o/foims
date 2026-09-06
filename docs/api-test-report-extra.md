# FOIMS API 补充集成测试报告

- 测试时间：2026-09-06 16:06:36 ~ 2026-09-06 16:06:38
- 服务端点：UDS /tmp/foims-dev.sock
- 覆盖分组：init状态 / 认证 / 日志 / 通知 / system配置 / 定时任务 / 双因素负例 / 会话

| 分组 | 用例 | 请求 | 状态码 | success | message | 判定 |
| init状态 | 初始化状态端点(初始化完成后404) | `GET /api/init/status` | 404 |  |  | ✅ |
| init状态 | 查询登录页初始化标志 | `GET /api/auth/init-status` | 200 | true | server.common.success | ✅ |
| 认证 | 查询可用登录方式 | `GET /api/auth/methods` | 200 | true | server.common.success | ✅ |
| 认证 | 获取验证码 | `GET /api/auth/captcha` | 200 | true | server.common.success | ✅ |
| 认证 | 登录空请求体(校验失败) | `POST /api/auth/login {}` | 400 |  | server.common.missing_field | ✅ |
| 认证 | 无令牌访问用户列表 | `GET /api/users(无cookie)` | 401 |  | server.auth.auth_failed | ✅ |
| 认证 | 管理员登录 | `POST /api/auth/login` | 200 | true | server.common.success | ✅ |
| 认证 | 查询当前用户 | `GET /api/auth/me` | 200 | true | server.common.success | ✅ |
| 日志 | 操作日志分页 | `GET /api/logs/operation` | 200 | true | server.logs.operation_retrieved | ✅ |
| 日志 | 登录日志分页 | `GET /api/logs/login` | 200 | true | server.logs.login_retrieved | ✅ |
| 通知 | 站内通知列表 | `GET /api/notifications` | 200 | true | server.notification.list_retrieved | ✅ |
| 通知 | 全部标记已读(幂等) | `PUT /api/notifications/mark-all-read` | 200 | true | server.notification.all_marked_read | ✅ |
| 日志 | 数据管理日志统计 | `GET /api/system/logs/stats` | 200 | true | server.logs.stats_retrieved | ✅ |
| 系统 | 系统信息 | `GET /api/system/info` | 200 | true | server.system.info_retrieved | ✅ |
| 系统 | 仪表盘统计 | `GET /api/system/dashboard-stats` | 200 | true | server.system.dashboard_stats_retrieved | ✅ |
| 系统 | 系统配置读取 | `GET /api/system/config` | 200 | true | server.system.config_retrieved | ✅ |
| 系统 | 支持语言列表 | `GET /api/system/languages` | 200 | true | server.system.languages_retrieved | ✅ |
| 系统 | 页面超时配置 | `GET /api/system/page-timeout` | 200 | true | server.system.page_timeout_retrieved | ✅ |
| 系统 | 会话超时配置 | `GET /api/system/session-timeout` | 200 | true | server.system.session_timeout_retrieved | ✅ |
| 系统 | 密码策略 | `GET /api/system/password-policy` | 200 | true | server.system.config_retrieved | ✅ |
| 系统 | 通知设置 | `GET /api/system/notification/settings` | 200 | true | server.notification.settings_retrieved | ✅ |
| 系统 | SMTP 配置读取 | `GET /api/system/smtp/config` | 200 | true | server.smtp.config_retrieved | ✅ |
| 系统 | LDAP 配置读取 | `GET /api/system/ldap/config` | 200 | true | server.ldap.config_retrieved | ✅ |
| 系统 | SSO 配置读取 | `GET /api/system/sso/config` | 200 | true | server.sso.config_retrieved | ✅ |
| 系统 | 应用层 fail2ban 状态 | `GET /api/system/fail2ban/app/status` | 200 | true | server.common.success | ✅ |
| 系统 | systemd 服务状态列表 | `GET /api/system/services` | 200 | true | server.services.status_retrieved | ✅ |
| 系统 | 证书列表 | `GET /api/system/certificate/list` | 200 | true | server.certificate.list_retrieved | ✅ |
| 证书 | CA 信息(公开,未生成时404) | `GET /api/certificate/ca/info` | 200 | true | server.certificate.list_retrieved | ✅ |
| 系统 | 界面语言写回 zh(幂等) | `PUT /api/system/language` | 200 | true | server.system.language_updated | ✅ |
| 系统 | 页面超时写回当前值(幂等) | `PUT /api/system/page-timeout` | 200 | true | server.system.page_timeout_updated | ✅ |
| 系统 | 会话超时写回当前值(幂等) | `PUT /api/system/session-timeout` | 200 | true | server.system.session_timeout_updated | ✅ |
| 定时任务 | 创建禁用态清理任务 | `POST /api/system/scheduled-tasks` | 201 | true | server.task.created | ✅ |
| 定时任务 | 任务列表 | `GET /api/system/scheduled-tasks` | 200 | true | server.task.list_retrieved | ✅ |
| 定时任务 | 任务执行日志 | `GET /api/system/scheduled-tasks/logs` | 200 | true | server.task.logs_retrieved | ✅ |
| 定时任务 | 删除测试任务 | `DELETE /api/system/scheduled-tasks/{id}` | 200 | true | server.task.deleted | ✅ |
| 双因素 | 未初始化直接启用(负例) | `POST /api/two-factor/enable` | 400 |  | server.common.missing_field | ✅ |
| 认证 | 刷新访问令牌 | `POST /api/auth/refresh` | 200 | true | server.common.success | ✅ |
| 认证 | 登出 | `POST /api/auth/logout` | 200 | true | server.common.success | ✅ |
| 认证 | 登出后访问当前用户(401) | `GET /api/auth/me(登出后)` | 401 |  | server.auth.token_revoked | ✅ |

## 汇总

- 用例总数：39
- 通过：39
- 失败：0
