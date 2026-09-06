#!/usr/bin/env bash
# =============================================================================
# FOIMS API 补充集成测试：覆盖 api_group_test.sh 未覆盖的分组
#（auth / 证书 CA / 日志 / 通知 / system 配置组 / 定时任务 / init 状态 / 2FA 负例）
# 说明：
#   - 通过 UDS(/tmp/foims-dev.sock) 直连本机 foims 服务；
#   - 只做只读查询与幂等/可回收写操作，不做破坏性操作
#    （不发邮件、不动服务、不封禁 IP、不关闭 init、不启用 2FA）；
#   - 结果写入 Markdown 报告。
# =============================================================================
set -u

SOCK="/tmp/foims-dev.sock"
BASE="http://localhost"
RUN="X$(date +%m%d%H%M)"
REPORT="/media/oi-io/AA709DF48A7AD5C2/foims/docs/api-test-report-extra.md"
ADMIN_COOKIE="/tmp/foims_admin_cookie_$RUN.txt"
RESP="/tmp/foims_resp_$RUN.json"
PASS_COUNT=0
FAIL_COUNT=0
TOTAL_COUNT=0
START_TS=$(date '+%Y-%m-%d %H:%M:%S')

curl_cmd() { curl -s --unix-socket "$SOCK" "$@"; }

req() {
  local method="$1" path="$2" body="${3:-}" jar="${4:-$ADMIN_COOKIE}"
  local args=(-o "$RESP" -w '%{http_code}' -X "$method" "$BASE$path" \
    -b "$jar" -H 'Content-Type: application/json' --max-time 90)
  if [ -n "$body" ]; then args+=(-d "$body"); fi
  HTTP_CODE=$(curl_cmd "${args[@]}")
  BODY=$(cat "$RESP" 2>/dev/null || echo '{}')
  SUCCESS=$(echo "$BODY" | jq -r '.success // empty' 2>/dev/null)
  MESSAGE=$(echo "$BODY" | jq -r '.message // empty' 2>/dev/null)
}

jget() { echo "$BODY" | jq -r "$1 // empty" 2>/dev/null; }

# 记录用例：组 用例名 期望(ok | ok_or_404 | 4xx | 401 | any2xx)
record() {
  local group="$1" name="$2" desc="$3" expect="$4"
  TOTAL_COUNT=$((TOTAL_COUNT + 1))
  local verdict="❌"
  case "$expect" in
    ok)    if [[ "$HTTP_CODE" == 2* && "$SUCCESS" == "true" ]]; then verdict="✅"; fi ;;
    ok_or_404)
           if [[ "$HTTP_CODE" == 2* && "$SUCCESS" == "true" ]] || [[ "$HTTP_CODE" == "404" ]]; then verdict="✅"; fi ;;
    4xx)   if [[ "$HTTP_CODE" == 4* ]]; then verdict="✅"; fi ;;
    401)   if [[ "$HTTP_CODE" == "401" ]]; then verdict="✅"; fi ;;
    any2xx) if [[ "$HTTP_CODE" == 2* ]]; then verdict="✅"; fi ;;
  esac
  if [ "$verdict" == "✅" ]; then PASS_COUNT=$((PASS_COUNT + 1)); else FAIL_COUNT=$((FAIL_COUNT + 1)); fi
  echo "| $group | $name | \`$desc\` | $HTTP_CODE | $SUCCESS | $MESSAGE | $verdict |"
}

ROWS_FILE="/tmp/api_rows_$RUN.md"
echo "| 分组 | 用例 | 请求 | 状态码 | success | message | 判定 |" > "$ROWS_FILE"
record_row() { echo "$1" >> "$ROWS_FILE"; }

R() { record "$@" >> "$ROWS_FILE"; }

# =============================================================================
# 组0：公开端点（无需认证）
# =============================================================================
# 注：/api/init/status 仅在初始化模式开启时挂载，已完成初始化的实例返回 404 属正确行为
req GET /api/init/status "" "/dev/null"
R "init状态" "初始化状态端点(初始化完成后404)" "GET /api/init/status" "ok_or_404"

req GET /api/auth/init-status "" "/dev/null"
R "init状态" "查询登录页初始化标志" "GET /api/auth/init-status" "ok"

req GET /api/auth/methods "" "/dev/null"
R "认证" "查询可用登录方式" "GET /api/auth/methods" "ok"

req GET /api/auth/captcha "" "/dev/null"
R "认证" "获取验证码" "GET /api/auth/captcha" "any2xx"

req POST /api/auth/login '{}' "" "/dev/null"
R "认证" "登录空请求体(校验失败)" "POST /api/auth/login {}" "4xx"

# 未认证访问受保护端点
req GET /api/users "" "/dev/null"
R "认证" "无令牌访问用户列表" "GET /api/users(无cookie)" "401"

# =============================================================================
# 组1：管理员登录（后续用例的会话来源）
# =============================================================================
req POST /api/auth/login '{"username":"admin","password":"admin123","remember_me":false}' "" "/dev/null"
R "认证" "管理员登录" "POST /api/auth/login" "ok"
# 登录响应同时下发了 cookie；用 -c 保存
curl_cmd -c "$ADMIN_COOKIE" -o /dev/null -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123","remember_me":false}'

req GET /api/auth/me
R "认证" "查询当前用户" "GET /api/auth/me" "ok"

# =============================================================================
# 组2：日志与通知
# =============================================================================
req GET "/api/logs/operation?page=1&page_size=5"
R "日志" "操作日志分页" "GET /api/logs/operation" "ok"

req GET "/api/logs/login?page=1&page_size=5"
R "日志" "登录日志分页" "GET /api/logs/login" "ok"

req GET /api/notifications
R "通知" "站内通知列表" "GET /api/notifications" "ok"

req PUT /api/notifications/mark-all-read '{}'
R "通知" "全部标记已读(幂等)" "PUT /api/notifications/mark-all-read" "ok"

req GET /api/system/logs/stats
R "日志" "数据管理日志统计" "GET /api/system/logs/stats" "ok"

# =============================================================================
# 组3：system 只读配置组
# =============================================================================
req GET /api/system/info
R "系统" "系统信息" "GET /api/system/info" "ok"

req GET /api/system/dashboard-stats
R "系统" "仪表盘统计" "GET /api/system/dashboard-stats" "ok"

req GET /api/system/config
R "系统" "系统配置读取" "GET /api/system/config" "ok"

req GET /api/system/languages
R "系统" "支持语言列表" "GET /api/system/languages" "ok"


req GET /api/system/page-timeout
R "系统" "页面超时配置" "GET /api/system/page-timeout" "ok"

req GET /api/system/session-timeout
R "系统" "会话超时配置" "GET /api/system/session-timeout" "ok"

req GET /api/system/password-policy
R "系统" "密码策略" "GET /api/system/password-policy" "ok"

req GET /api/system/notification/settings
R "系统" "通知设置" "GET /api/system/notification/settings" "ok"

req GET /api/system/smtp/config
R "系统" "SMTP 配置读取" "GET /api/system/smtp/config" "ok"

req GET /api/system/ldap/config
R "系统" "LDAP 配置读取" "GET /api/system/ldap/config" "ok"

req GET /api/system/sso/config
R "系统" "SSO 配置读取" "GET /api/system/sso/config" "ok"

req GET /api/system/fail2ban/app/status
R "系统" "应用层 fail2ban 状态" "GET /api/system/fail2ban/app/status" "ok"

req GET /api/system/services
R "系统" "systemd 服务状态列表" "GET /api/system/services" "ok"

req GET /api/system/certificate/list
R "系统" "证书列表" "GET /api/system/certificate/list" "ok"

# CA 公开信息（未生成 CA 时 404 属正确行为）
req GET /api/certificate/ca/info "" "/dev/null"
R "证书" "CA 信息(公开,未生成时404)" "GET /api/certificate/ca/info" "ok_or_404"

# =============================================================================
# 组4：system 幂等写（写回当前值）
# =============================================================================
req PUT /api/system/language '{"language":"zh"}'
R "系统" "界面语言写回 zh(幂等)" "PUT /api/system/language" "ok"

req GET /api/system/page-timeout
CUR_PT=$(jget '.data.page_timeout')
req PUT /api/system/page-timeout "{\"page_timeout\":${CUR_PT:-30}}"
R "系统" "页面超时写回当前值(幂等)" "PUT /api/system/page-timeout" "ok"

req GET /api/system/session-timeout
CUR_ST=$(jget '.data.session_timeout')
req PUT /api/system/session-timeout "{\"session_timeout\":${CUR_ST:-1440}}"
R "系统" "会话超时写回当前值(幂等)" "PUT /api/system/session-timeout" "ok"

# =============================================================================
# 组5：定时任务（创建→查询→删除，全程 disabled 不触发执行）
# =============================================================================
req POST /api/system/scheduled-tasks \
  '{"name":"api-extra-测试任务-'"$RUN"'","task_type":"log_cleanup","cron_expression":"0 4 1 * *","enabled":false,"config":{"days":365}}'
TASK_ID=$(jget '.data.id')
R "定时任务" "创建禁用态清理任务" "POST /api/system/scheduled-tasks" "ok"

req GET /api/system/scheduled-tasks
R "定时任务" "任务列表" "GET /api/system/scheduled-tasks" "ok"

req GET /api/system/scheduled-tasks/logs
R "定时任务" "任务执行日志" "GET /api/system/scheduled-tasks/logs" "ok"

if [ -n "$TASK_ID" ]; then
  req DELETE "/api/system/scheduled-tasks/$TASK_ID"
  R "定时任务" "删除测试任务" "DELETE /api/system/scheduled-tasks/{id}" "ok"
else
  R "定时任务" "删除测试任务" "跳过(未取到 id)" "4xx"
fi

# =============================================================================
# 组6：2FA 负例（未初始化直接启用应被拒绝）
# =============================================================================
req POST /api/two-factor/enable '{}'
R "双因素" "未初始化直接启用(负例)" "POST /api/two-factor/enable" "4xx"

# =============================================================================
# 组7：会话收尾（刷新→登出→失效验证）
# =============================================================================
req POST /api/auth/refresh
R "认证" "刷新访问令牌" "POST /api/auth/refresh" "ok"

req POST /api/auth/logout
R "认证" "登出" "POST /api/auth/logout" "ok"

req GET /api/auth/me
R "认证" "登出后访问当前用户(401)" "GET /api/auth/me(登出后)" "401"

# =============================================================================
END_TS=$(date '+%Y-%m-%d %H:%M:%S')
{
  echo "# FOIMS API 补充集成测试报告"
  echo ""
  echo "- 测试时间：$START_TS ~ $END_TS"
  echo "- 服务端点：UDS $SOCK"
  echo "- 覆盖分组：init状态 / 认证 / 日志 / 通知 / system配置 / 定时任务 / 双因素负例 / 会话"
  echo ""
  cat "$ROWS_FILE"
  echo ""
  echo "## 汇总"
  echo ""
  echo "- 用例总数：$TOTAL_COUNT"
  echo "- 通过：$PASS_COUNT"
  echo "- 失败：$FAIL_COUNT"
} > "$REPORT"

echo ""
echo "=============================="
echo "补充用例: $TOTAL_COUNT  通过: $PASS_COUNT  失败: $FAIL_COUNT"
echo "报告: $REPORT"
