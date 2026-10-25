#!/usr/bin/env bash
# FOIMS 全量 API 冒烟测试：登录 admin 后遍历所有 GET 端点 + 鉴权负例 + CRUD 闭环 + 写端点校验探针
# 判定规则：
#   - 常规 GET：期望 2xx，否则 FAIL
#   - 动态 {id} GET / 探针 POST/PUT/DELETE：期望非 5xx（路由与鉴权可达即可），429/5xx 记 FAIL
#   - 未认证负例：期望 401
set -u
BASE="${BASE:-http://localhost}"
JAR="$(mktemp)"; BODY="$(mktemp)"; LOG="$(mktemp)"
FAIL=0; TOTAL=0
UUID_NIL="00000000-0000-0000-0000-000000000000"

# record 状态码 方法 路径 判定结果 备注
record() {
  TOTAL=$((TOTAL+1))
  printf '%-6s %-4s %-70s %s\n' "$1" "$2" "$3" "$4" | tee -a "$LOG"
  case "$4" in FAIL*) FAIL=$((FAIL+1));; esac
}

# check_get 方法 路径 [need_auth]
check_get() {
  local method="$1" path="$2" auth="${3:-1}" expect_ok="${4:-2}"
  local args=(-s -o "$BODY" -w '%{http_code}' -X "$method" "$BASE$path")
  [ "$auth" = "1" ] && args+=(-b "$JAR" -c "$JAR")
  local code; code=$(curl "${args[@]}" -H 'Content-Type: application/json' --max-time 20)
  local verdict
  if [ "$expect_ok" = "2" ]; then
    case "$code" in 2??) verdict="PASS";; 429) verdict="FAIL(rate429)";; *) verdict="FAIL";; esac
  else
    case "$code" in 5??|000|429) verdict="FAIL";; *) verdict="PASS(code=$code)";; esac
  fi
  record "$code" "$method" "$path" "$verdict"
}

# first_id jq路径提取列表响应首个 id（兼容 data.items / data 数组 / data 对象）
first_id() {
  local path="$1"
  curl -s -b "$JAR" --max-time 20 "$BASE$path" \
    | jq -r '[.. | objects | .id? // empty] | .[0] // empty' 2>/dev/null
}

post_json() { # 方法 路径 json体
  curl -s -o "$BODY" -w '%{http_code}' -X "$1" -b "$JAR" -c "$JAR" \
    -H 'Content-Type: application/json' -d "$3" --max-time 25 "$BASE$2"
}

echo "=== 1. 登录（admin） ==="
code=$(post_json POST /api/auth/login '{"username":"admin","password":"admin123","remember_me":false}')
if [ "$code" != "200" ]; then echo "登录失败 code=$code，响应:"; cat "$BODY"; exit 1; fi
echo "登录成功 code=$code"
record "$code" POST /api/auth/login PASS

echo; echo "=== 2. 公开端点（无认证） ==="
for p in /health /api/auth/init-status /api/auth/methods /api/auth/captcha \
         /api/certificate/ca/info /api/certificate/ca/download; do
  check_get GET "$p" 0
done
check_get GET "/api/auth/sso/login" 0 5   # 未配置 SSO 时预期 4xx，非 5xx 即可

echo; echo "=== 3. 鉴权负例（未带令牌应 401） ==="
for p in /api/auth/me /api/users /api/resources/devices /api/system/config \
         /api/logs/login /api/agents /api/system/scheduled-tasks; do
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "$BASE$p")
  [ "$code" = "401" ] && verdict=PASS || verdict="FAIL(want 401)"
  record "$code" GET "$p (no-auth)" "$verdict"
done

echo; echo "=== 4. 会话与刷新 ==="
check_get GET /api/auth/me
check_get GET /api/resources/options/rooms
check_get POST /api/auth/refresh   # 用 refresh cookie 换新 access cookie（后续沿用新 jar）

echo; echo "=== 5. 列表类 GET（期望 2xx） ==="
for p in \
  /api/users \
  /api/resources/networks /api/resources/network-regions /api/resources/rooms \
  /api/resources/cabinets /api/resources/workstations /api/resources/positions \
  /api/resources/ip/pull-defaults /api/resources/topology/nodes \
  /api/resources/topology/connections /api/resources/organizations \
  /api/resources/organizations/tree /api/resources/employees \
  /api/resources/org-templates /api/resources/org-templates/available-types \
  /api/resources/net-outlets /api/resources/patch-panels /api/resources/cable-links \
  /api/resources/device-templates /api/resources/devices /api/resources/devices/interfaces \
  /api/snmp/events /api/logs/operation /api/logs/login /api/notifications \
  /api/system/info /api/system/services /api/system/dashboard-stats \
  /api/system/smtp/config /api/system/ldap/config /api/system/sso/config \
  /api/system/config /api/system/certificate/list /api/system/languages \
  /api/system/page-timeout /api/system/session-timeout \
  /api/system/notification/settings /api/system/password-policy \
  /api/system/logs/stats /api/system/logs/forwarding \
  /api/system/scheduled-tasks /api/system/scheduled-tasks/logs \
  /api/system/fail2ban/app/status \
  /api/agents /api/agents/top-metrics /api/agents/alert-thresholds \
  /api/agents/enrollment-token \
  /api/system/config/backup \
  /api/system/import-export/export/csv \
  /api/system/import-export/template \
; do check_get GET "$p"; done

echo; echo "=== 6. 动态 {id} GET（id 取自列表，缺省用 NIL UUID，非 5xx 即 PASS） ==="
NET_ID=$(first_id /api/resources/networks);        NET_ID=${NET_ID:-$UUID_NIL}
REG_ID=$(first_id /api/resources/network-regions); REG_ID=${REG_ID:-$UUID_NIL}
ROOM_ID=$(first_id /api/resources/rooms);          ROOM_ID=${ROOM_ID:-$UUID_NIL}
CAB_ID=$(first_id /api/resources/cabinets);        CAB_ID=${CAB_ID:-$UUID_NIL}
WS_ID=$(first_id /api/resources/workstations);     WS_ID=${WS_ID:-$UUID_NIL}
POS_ID=$(first_id /api/resources/positions);       POS_ID=${POS_ID:-$UUID_NIL}
ORG_ID=$(first_id /api/resources/organizations);   ORG_ID=${ORG_ID:-$UUID_NIL}
EMP_ID=$(first_id /api/resources/employees);       EMP_ID=${EMP_ID:-$UUID_NIL}
TPL_ID=$(first_id /api/resources/org-templates);   TPL_ID=${TPL_ID:-$UUID_NIL}
OUT_ID=$(first_id /api/resources/net-outlets);     OUT_ID=${OUT_ID:-$UUID_NIL}
CBL_ID=$(first_id /api/resources/cable-links);     CBL_ID=${CBL_ID:-$UUID_NIL}
DEV_ID=$(first_id /api/resources/devices);         DEV_ID=${DEV_ID:-$UUID_NIL}
USR_ID=$(first_id /api/users);                     USR_ID=${USR_ID:-$UUID_NIL}
TSK_ID=$(first_id /api/system/scheduled-tasks);    TSK_ID=${TSK_ID:-$UUID_NIL}
AGT_ID=$(first_id /api/agents);                    AGT_ID=${AGT_ID:-$UUID_NIL}
NFT_ID=$(first_id /api/resources/device-templates); NFT_ID=${NFT_ID:-1}

for p in \
  "/api/users/$USR_ID" \
  "/api/resources/networks/$NET_ID" \
  "/api/resources/network-regions/$REG_ID" \
  "/api/resources/network-regions/$REG_ID/cabinets" \
  "/api/resources/rooms/$ROOM_ID" \
  "/api/resources/rooms/$ROOM_ID/brief" \
  "/api/resources/rooms/$ROOM_ID/networks" \
  "/api/resources/cabinets/$CAB_ID" \
  "/api/resources/cabinets/$CAB_ID/networks" \
  "/api/resources/workstations/$WS_ID" \
  "/api/resources/positions/$POS_ID" \
  "/api/resources/ip/available/$NET_ID" \
  "/api/resources/layouts/workstation/$ROOM_ID" \
  "/api/resources/layouts/positions/$ROOM_ID" \
  "/api/resources/layouts/room-cabinets/$ROOM_ID" \
  "/api/resources/organizations/$ORG_ID" \
  "/api/resources/organizations/$ORG_ID/children" \
  "/api/resources/organizations/$ORG_ID/allowed-child-types" \
  "/api/resources/organizations/$ORG_ID/rooms" \
  "/api/resources/employees/$EMP_ID" \
  "/api/resources/org-templates/$TPL_ID" \
  "/api/resources/net-outlets/$OUT_ID" \
  "/api/resources/cable-links/$CBL_ID" \
  "/api/resources/cable-links/path" \
  "/api/resources/device-templates/$NFT_ID" \
  "/api/resources/devices/$DEV_ID" \
  "/api/resources/devices/$DEV_ID/nics" \
  "/api/resources/devices/$DEV_ID/ips" \
  "/api/resources/devices/$DEV_ID/macs" \
  "/api/resources/devices/$DEV_ID/lldp-neighbors" \
  "/api/resources/devices/$DEV_ID/entities" \
  "/api/resources/devices/$DEV_ID/sensors" \
  "/api/resources/devices/$DEV_ID/vlans" \
  "/api/resources/devices/$DEV_ID/cdp" \
  "/api/resources/devices/$DEV_ID/lag" \
  "/api/resources/devices/$DEV_ID/poe" \
  "/api/resources/devices/$DEV_ID/interfaces" \
  "/api/system/scheduled-tasks/$TSK_ID" \
  "/api/agents/$AGT_ID" \
  "/api/agents/$AGT_ID/history" \
  "/api/snmp/locate" \
  "/api/agents/config-package?format=zip" \
; do check_get GET "$p" 1 5; done

echo; echo "=== 7. CRUD 闭环（网络区域：创建→查询→更新→删除） ==="
TS=$(date +%s); REGION_NAME="smoke-$((TS % 100000))"
code=$(post_json POST /api/resources/network-regions \
  "{\"name\":\"$REGION_NAME\",\"description\":\"API 冒烟测试临时区域\",\"ipv4_cidrs\":[\"10.98.76.0/24\"]}")
if [ "$code" = "200" ] || [ "$code" = "201" ]; then
  record "$code" POST /api/resources/network-regions PASS
  NEW_ID=$(jq -r '.. | .id? // empty' "$BODY" | head -1)
  NEW_ID=${NEW_ID:-$UUID_NIL}
  code=$(curl -s -o /dev/null -w '%{http_code}' -b "$JAR" "$BASE/api/resources/network-regions/$NEW_ID")
  case "$code" in 2??) record "$code" GET "/api/resources/network-regions/{id}" PASS;; *) record "$code" GET "/api/resources/network-regions/{id}" FAIL;; esac
  code=$(post_json PUT "/api/resources/network-regions/$NEW_ID" \
    "{\"name\":\"$REGION_NAME\",\"description\":\"API 冒烟测试临时区域（已更新）\",\"ipv4_cidrs\":[\"10.98.76.0/25\"]}")
  case "$code" in 2??) record "$code" PUT "/api/resources/network-regions/{id}" PASS;; *) record "$code" PUT "/api/resources/network-regions/{id}" FAIL;; esac
  code=$(curl -s -o /dev/null -w '%{http_code}' -X DELETE -b "$JAR" "$BASE/api/resources/network-regions/$NEW_ID")
  case "$code" in 2??) record "$code" DELETE "/api/resources/network-regions/{id}" PASS;; *) record "$code" DELETE "/api/resources/network-regions/{id}" FAIL;; esac
else
  record "$code" POST /api/resources/network-regions "FAIL(create，跳过后续)"
fi

echo; echo "=== 8. 写端点校验探针（空载荷应 4xx 拦截，非 5xx） ==="
for p in /api/users /api/resources/networks /api/resources/rooms /api/resources/devices \
         /api/resources/organizations /api/resources/employees /api/resources/cabinets \
         /api/resources/workstations /api/resources/net-outlets /api/resources/cable-links \
         /api/resources/positions /api/resources/org-templates \
         /api/resources/ip/batch /api/resources/ip/auto-assign \
         /api/resources/devices/test-snmp /api/resources/topology/nodes \
         /api/resources/topology/connections /api/auth/change-password \
         /api/system/scheduled-tasks /api/system/smtp/send /api/system/ldap/test \
         /api/system/sso/test /api/system/logs/forwarding/test \
         /api/agents/alert-thresholds; do
  code=$(post_json POST "$p" '{}')
  case "$code" in 5??|000|429) record "$code" POST "$p" FAIL;; *) record "$code" POST "$p" "PASS(code=$code)";; esac
done
for p in /api/system/smtp/config /api/system/config /api/system/page-timeout \
         /api/system/session-timeout /api/system/notification/settings \
         /api/system/password-policy /api/system/language \
         /api/system/fail2ban/app/config /api/system/logs/forwarding; do
  code=$(post_json PUT "$p" '{}')
  case "$code" in 5??|000|429) record "$code" PUT "$p" FAIL;; *) record "$code" PUT "$p" "PASS(code=$code)";; esac
done

echo; echo "=== 9. 登出 ==="
code=$(post_json POST /api/auth/logout '{}')
case "$code" in 2??) record "$code" POST /api/auth/logout PASS;; *) record "$code" POST /api/auth/logout FAIL;; esac
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$JAR" "$BASE/api/auth/me")
[ "$code" = "401" ] && record "$code" GET "/api/auth/me (post-logout)" PASS \
  || record "$code" GET "/api/auth/me (post-logout)" "FAIL(want 401)"

echo
echo "================ 汇总 ================"
echo "总用例: $TOTAL   失败: $FAIL"
grep -n 'FAIL' "$LOG" || echo "无失败用例"
rm -f "$JAR" "$BODY" "$LOG"
[ "$FAIL" -eq 0 ]
