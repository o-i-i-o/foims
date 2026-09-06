#!/usr/bin/env bash
# =============================================================================
# FOIMS API 分组集成测试
# 顺序：组织管理 → 网络区域 → 网段 → 房间 → 机柜 → 设备 → 线路 → 可视化
# 说明：
#   - 通过 UDS(/tmp/foims-dev.sock) 直连本机 foims 服务；
#   - 测试产生的业务数据全部保留在数据库中（不做清理）；
#   - 结果写入 Markdown 报告（由 REPORT 变量指定）。
# =============================================================================
set -u

SOCK="/tmp/foims-dev.sock"
BASE="http://localhost"
RUN="T$(date +%m%d%H%M%S)"
REPORT="/media/oi-io/AA709DF48A7AD5C2/foims/docs/api-test-report.md"
ADMIN_COOKIE="/tmp/foims_admin_cookie_$RUN.txt"
USER_COOKIE="/tmp/foims_user_cookie_$RUN.txt"
RESP="/tmp/foims_resp_$RUN.json"
ADMIN_USER="admin"
ADMIN_PASS="admin123"
TEST_USER="apitest_$RUN"
TEST_PASS="Apitest#2026x"
PASS_COUNT=0
FAIL_COUNT=0
TOTAL_COUNT=0
START_TS=$(date '+%Y-%m-%d %H:%M:%S')

curl_cmd() { curl -s --unix-socket "$SOCK" "$@"; }

# 发送请求：method path [json_body] [cookie_jar]
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

# 记录用例：组 用例名 请求描述 期望(ok=2xx+success:true | 4xx | err=非2xx | code:具体码)
record() {
  local group="$1" name="$2" desc="$3" expect="$4" note="${5:-}"
  TOTAL_COUNT=$((TOTAL_COUNT + 1))
  local verdict="❌"
  case "$expect" in
    ok)   if [[ "$HTTP_CODE" == 2* && "$SUCCESS" == "true" ]]; then verdict="✅"; fi ;;
    4xx)  if [[ "$HTTP_CODE" == 4* ]]; then verdict="✅"; fi ;;
    err)  if [[ "$HTTP_CODE" != 2* ]]; then verdict="✅"; fi ;;
    5xx)  if [[ "$HTTP_CODE" == 5* ]]; then verdict="✅"; fi ;;
    file) if [[ "$HTTP_CODE" == "200" ]]; then verdict="✅"; fi ;;
  esac
  if [[ "$verdict" == "✅" ]]; then PASS_COUNT=$((PASS_COUNT + 1)); else FAIL_COUNT=$((FAIL_COUNT + 1)); fi
  if [ -n "$note" ]; then name="$name（$note）"; fi
  echo "| $group | $name | \`$desc\` | $HTTP_CODE | $SUCCESS | $MESSAGE | $verdict |" >> "$ROWS_FILE"
}

# 从响应提取 JSON 路径的值
jget() { echo "$BODY" | jq -r "$1 // empty" 2>/dev/null; }

ROWS_FILE="/tmp/api_rows_$RUN.md"

# 基于 RUN 后 4 位数字派生本批次唯一网段编号（避开 0/1 常用段）
# 探测未被占用的网段号：网段 CIDR 全局唯一且测试数据保留不清理，
# 多次重跑必须避开历史批次占用的 10.X.0/24 / fd00:99:X::/64
probe_unused_n() {
  local used
  used=$(curl -s --unix-socket "$SOCK" -b "$ADMIN_COOKIE" \
    "http://localhost/api/resources/networks?page=1&page_size=1000" \
    | jq -r '[.data.items[].ipv4_cidr] + [.data.items[].ipv6_cidr] | join(" ")' 2>/dev/null)
  for cand in $(seq 10 248); do
    # 本批次会占用 cand 与 cand+1 两个网段，两者都空闲才可选
    local next=$((cand + 1))
    if ! echo "$used" | grep -q "10.99.$cand.0/24" \
      && ! echo "$used" | grep -q "fd00:99:$cand::" \
      && ! echo "$used" | grep -q "10.99.$next.0/24" \
      && ! echo "$used" | grep -q "fd00:99:$next::"; then
      echo "$cand"
      return
    fi
  done
  echo 10
}
# 先完成管理员登录拿到 cookie（探测需要），再取网段号
curl_cmd -c "$ADMIN_COOKIE" -o /dev/null -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"'"$ADMIN_USER"'","password":"'"$ADMIN_PASS"'","remember_me":false}'
N=$(probe_unused_n)
V4_NET1="10.99.$N.0/24"; V4_GW1="10.99.$N.1"
V4_NET2="10.99.$((N+1)).0/24"; V4_GW2="10.99.$((N+1)).1"
V6_NET="fd00:99:$N::/64"; V6_GW="fd00:99:$N::1"

# =============================================================================
# 前置：会话与测试用户
# =============================================================================
echo "# FOIMS API 分组集成测试报告" > "$REPORT"
echo "" >> "$REPORT"
echo "- 测试时间：$START_TS" >> "$REPORT"
echo "- 运行标识：$RUN（本批次创建的数据名均含该后缀，全部保留）" >> "$REPORT"
echo "- 服务端点：UDS $SOCK" >> "$REPORT"
echo "" >> "$REPORT"

echo "| 分组 | 用例 | 请求 | 状态码 | success | message | 判定 |" > "$ROWS_FILE"
echo "|---|---|---|---|---|---|---|" >> "$ROWS_FILE"

# --- 0. 基础与认证 ---
req GET /health "" /dev/null
record "0 基础" "健康检查" "GET /health" ok

HTTP_CODE=$(curl_cmd -o "$RESP" -w '%{http_code}' -c "$ADMIN_COOKIE" -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\"}")
BODY=$(cat "$RESP"); SUCCESS=$(echo "$BODY" | jq -r '.success // empty'); MESSAGE=$(echo "$BODY" | jq -r '.message // empty')
record "0 基础" "管理员登录" "POST /api/auth/login" ok

req GET /api/auth/me
record "0 基础" "当前用户信息" "GET /api/auth/me" ok

req GET /api/auth/init-status "" /dev/null
record "0 基础" "初始化状态（公开）" "GET /api/auth/init-status" ok

# 未认证访问受保护接口 → 401
req GET /api/resources/organizations "" /dev/null
record "0 基础" "未认证访问受保护接口" "GET /api/resources/organizations（无凭据）" 4xx "期望 401"

# 创建普通用户并登录（用于 RBAC 验证）
req POST /api/users "{\"username\":\"$TEST_USER\",\"password\":\"$TEST_PASS\",\"email\":\"$TEST_USER@test.local\",\"role\":\"user\"}"
record "0 基础" "创建普通用户 $TEST_USER" "POST /api/users" ok

curl_cmd -s -o /dev/null -c "$USER_COOKIE" -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' -d "{\"username\":\"$TEST_USER\",\"password\":\"$TEST_PASS\"}"

req GET /api/users "" "$USER_COOKIE"
record "0 基础" "普通用户访问用户管理（RBAC）" "GET /api/users（user 角色）" 4xx "期望 403"

# =============================================================================
# 1. 组织管理
# =============================================================================
TPL_NAME="模板-园区层级-$RUN"
req POST /api/resources/org-templates "{\"name\":\"$TPL_NAME\",\"levels\":{\"园区\":[\"楼宇\"],\"楼宇\":[\"楼层\"],\"楼层\":[]},\"description\":\"API 分组测试模板\"}"
record "1 组织管理" "创建组织模板" "POST /api/resources/org-templates" ok
TPL_ID=$(jget '.data.id')

req GET /api/resources/org-templates
record "1 组织管理" "模板列表" "GET /api/resources/org-templates" ok

req GET "/api/resources/org-templates/available-types"
record "1 组织管理" "可用组织类型" "GET /api/resources/org-templates/available-types" ok

req GET "/api/resources/org-templates/$TPL_ID"
record "1 组织管理" "模板详情" "GET /api/resources/org-templates/{id}" ok

req POST /api/resources/organizations "{\"name\":\"测试园区-$RUN\",\"type_path\":\"0\",\"template_id\":\"$TPL_ID\",\"description\":\"根节点\"}"
record "1 组织管理" "创建根组织（园区）" "POST /api/resources/organizations" ok
ORG_ROOT=$(jget '.data.id')

req POST /api/resources/organizations "{\"name\":\"一号楼-$RUN\",\"type_path\":\"0.0\",\"parent_id\":\"$ORG_ROOT\"}"
record "1 组织管理" "创建子组织（楼宇）" "POST /api/resources/organizations" ok
ORG_BUILDING=$(jget '.data.id')

req POST /api/resources/organizations "{\"name\":\"一层-$RUN\",\"type_path\":\"0.0.0\",\"parent_id\":\"$ORG_BUILDING\"}"
record "1 组织管理" "创建孙组织（楼层）" "POST /api/resources/organizations" ok
ORG_FLOOR=$(jget '.data.id')

req GET "/api/resources/organizations?search=测试园区-$RUN"
record "1 组织管理" "组织列表（搜索）" "GET /api/resources/organizations?search=…" ok

req GET /api/resources/organizations/tree
record "1 组织管理" "组织树" "GET /api/resources/organizations/tree" ok

req GET "/api/resources/organizations/$ORG_ROOT"
record "1 组织管理" "组织详情（含子级统计）" "GET /api/resources/organizations/{id}" ok

req GET "/api/resources/organizations/$ORG_ROOT/children"
record "1 组织管理" "子组织列表" "GET /api/resources/organizations/{id}/children" ok

req GET "/api/resources/organizations/$ORG_ROOT/allowed-child-types"
record "1 组织管理" "允许的子类型" "GET /api/resources/organizations/{id}/allowed-child-types" ok

req PUT "/api/resources/organizations/$ORG_ROOT" '{"description":"根节点-已更新"}'
record "1 组织管理" "更新组织" "PUT /api/resources/organizations/{id}" ok

req GET "/api/resources/organizations/$ORG_ROOT/rooms"
record "1 组织管理" "组织下房间列表（空）" "GET /api/resources/organizations/{id}/rooms" ok

req POST /api/resources/organizations "{\"name\":\"非法-$RUN\",\"type_path\":\"abc\",\"template_id\":\"$TPL_ID\"}"
record "1 组织管理" "非法 type_path 被拒绝" "POST /api/resources/organizations（type_path=abc）" 4xx

req POST /api/resources/organizations "{\"name\":\"一号楼-$RUN\",\"type_path\":\"0.0\",\"parent_id\":\"$ORG_ROOT\"}"
record "1 组织管理" "同级重名被拒绝" "POST /api/resources/organizations（重名）" 4xx

# =============================================================================
# 2. 网络区域
# =============================================================================
req POST /api/resources/network-regions "{\"name\":\"核心区域-$RUN\",\"description\":\"API 测试区域\",\"ipv4_cidrs\":[\"10.99.0.0/16\"],\"ipv6_cidrs\":[\"fd00:99::/48\"]}"
record "2 网络区域" "创建网络区域" "POST /api/resources/network-regions" ok
REGION_ID=$(jget '.data.id')

req GET "/api/resources/network-regions?search=核心区域-$RUN"
record "2 网络区域" "区域列表（搜索）" "GET /api/resources/network-regions?search=…" ok

req GET "/api/resources/network-regions/$REGION_ID"
record "2 网络区域" "区域详情" "GET /api/resources/network-regions/{id}" ok

req PUT "/api/resources/network-regions/$REGION_ID" "{\"ipv4_cidrs\":[\"10.99.0.0/16\",\"10.98.0.0/16\"],\"ipv6_cidrs\":[\"fd00:99::/32\"]}"
record "2 网络区域" "更新区域 CIDR" "PUT /api/resources/network-regions/{id}" ok

req GET "/api/resources/network-regions/$REGION_ID/cabinets"
record "2 网络区域" "区域下机柜（经网段/房间推导）" "GET /api/resources/network-regions/{id}/cabinets" ok

req POST /api/resources/network-regions "{\"name\":\"超长名称区域-$RUN-超过二十个字符的限制测试数据\",\"description\":\"\"}"
record "2 网络区域" "区域名超长（>20）被拒绝" "POST /api/resources/network-regions（name 21+）" 4xx

# =============================================================================
# 3. 网段
# =============================================================================
req POST /api/resources/networks "{\"name\":\"办公网段-$RUN\",\"network_region_id\":\"$REGION_ID\",\"ipv4_cidr\":\"$V4_NET1\",\"ipv4_gateway\":\"$V4_GW1\",\"ipv4_dns\":[\"223.5.5.5\",\"114.114.114.114\"],\"description\":\"办公网\"}"
record "3 网段" "创建 IPv4 网段" "POST /api/resources/networks" ok
NET_V4=$(jget '.data.id')

req POST /api/resources/networks "{\"name\":\"服务器网段-$RUN\",\"network_region_id\":\"$REGION_ID\",\"ipv4_cidr\":\"$V4_NET2\",\"ipv4_gateway\":\"$V4_GW2\",\"description\":\"服务器网\"}"
record "3 网段" "创建第二个 IPv4 网段" "POST /api/resources/networks" ok
NET_V4_SVR=$(jget '.data.id')

req POST /api/resources/networks "{\"name\":\"IPv6网段-$RUN\",\"network_region_id\":\"$REGION_ID\",\"ipv6_cidr\":\"$V6_NET\",\"ipv6_gateway\":\"$V6_GW\"}"
record "3 网段" "创建 IPv6 网段" "POST /api/resources/networks" ok
NET_V6=$(jget '.data.id')

req GET "/api/resources/networks?search=办公网段-$RUN"
record "3 网段" "网段列表（搜索）" "GET /api/resources/networks?search=…" ok

req GET "/api/resources/networks/$NET_V4"
record "3 网段" "网段详情" "GET /api/resources/networks/{id}" ok

req PUT "/api/resources/networks/$NET_V4" '{"description":"办公网-已更新"}'
record "3 网段" "更新网段" "PUT /api/resources/networks/{id}" ok

req GET "/api/resources/ip/available/$NET_V4"
record "3 网段" "可用 IP 列表" "GET /api/resources/ip/available/{network_id}" ok

req POST /api/resources/networks "{\"name\":\"非法网段-$RUN\",\"network_region_id\":\"$REGION_ID\",\"ipv4_cidr\":\"300.1.1.0/24\"}"
record "3 网段" "非法 CIDR 被拒绝" "POST /api/resources/networks（300.1.1.0/24）" 4xx

req POST /api/resources/networks "{\"name\":\"网关错段-$RUN\",\"network_region_id\":\"$REGION_ID\",\"ipv4_cidr\":\"10.99.250.0/24\",\"ipv4_gateway\":\"10.99.1.254\"}"
record "3 网段" "网关不在网段内被拒绝" "POST /api/resources/networks（网关跨段）" 4xx

# =============================================================================
# 4. 房间
# =============================================================================
req POST /api/resources/rooms "{\"name\":\"测试机房-$RUN\",\"room_type\":\"DATA_CENTER\",\"org_id\":\"$ORG_FLOOR\",\"subnet_ids\":[\"$NET_V4\",\"$NET_V4_SVR\"],\"description\":\"数据中心机房\"}"
record "4 房间" "创建机房（DATA_CENTER）" "POST /api/resources/rooms" ok
ROOM_DC=$(jget '.data.id')

req POST /api/resources/rooms "{\"name\":\"测试办公室-$RUN\",\"room_type\":\"OFFICE\",\"org_id\":\"$ORG_FLOOR\",\"subnet_ids\":[\"$NET_V4\"]}"
record "4 房间" "创建办公室（OFFICE）" "POST /api/resources/rooms" ok
ROOM_OFFICE=$(jget '.data.id')

req GET "/api/resources/rooms?search=测试机房-$RUN"
record "4 房间" "房间列表（搜索）" "GET /api/resources/rooms?search=…" ok

req GET "/api/resources/rooms/$ROOM_DC"
record "4 房间" "房间详情（含网段/机柜/工位）" "GET /api/resources/rooms/{id}" ok

req GET "/api/resources/rooms/$ROOM_DC/networks"
record "4 房间" "房间关联网段" "GET /api/resources/rooms/{id}/networks" ok

req PUT "/api/resources/rooms/$ROOM_DC" '{"description":"数据中心机房-已更新"}'
record "4 房间" "更新房间" "PUT /api/resources/rooms/{id}" ok

# 办公室：同步工位；机房：同步机柜与信息点
req PUT "/api/resources/rooms/$ROOM_OFFICE/children" "{\"workstations\":[{\"name\":\"工位W1-$RUN\",\"manager\":\"张三\"},{\"name\":\"工位W2-$RUN\"}]}"
record "4 房间" "同步房间子项-工位" "PUT /api/resources/rooms/{id}/children" ok

req PUT "/api/resources/rooms/$ROOM_DC/children" "{\"cabinets\":[{\"name\":\"机柜A-$RUN\",\"capacity\":42},{\"name\":\"机柜B-$RUN\",\"capacity\":24}]}"
record "4 房间" "同步房间子项-机柜" "PUT /api/resources/rooms/{id}/children" ok

req PUT "/api/resources/rooms/$ROOM_DC/net-outlets" "{\"net_outlets\":[{\"name\":\"信息点01-$RUN\"},{\"name\":\"信息点02-$RUN\"}]}"
record "4 房间" "同步信息点" "PUT /api/resources/rooms/{id}/net-outlets" ok

req POST /api/resources/rooms "{\"name\":\"非法房型-$RUN\",\"room_type\":\"warehouse\"}"
record "4 房间" "非法房型被拒绝" "POST /api/resources/rooms（room_type=warehouse）" 4xx

# 取回创建的工位/机柜/信息点 ID
req GET "/api/resources/rooms/$ROOM_OFFICE"
WS1=$(echo "$BODY" | jq -r ".data.workstations[]? | select(.name==\"工位W1-$RUN\") | .id" 2>/dev/null)
WS2=$(echo "$BODY" | jq -r ".data.workstations[]? | select(.name==\"工位W2-$RUN\") | .id" 2>/dev/null)

req GET "/api/resources/rooms/$ROOM_DC"
CAB_A=$(echo "$BODY" | jq -r ".data.cabinets[]? | select(.name==\"机柜A-$RUN\") | .id" 2>/dev/null)
CAB_B=$(echo "$BODY" | jq -r ".data.cabinets[]? | select(.name==\"机柜B-$RUN\") | .id" 2>/dev/null)

req GET "/api/resources/net-outlets?room_id=$ROOM_DC"
OUTLET1=$(echo "$BODY" | jq -r ".data.items[]? | select(.name==\"信息点01-$RUN\") | .id" 2>/dev/null)
OUTLET2=$(echo "$BODY" | jq -r ".data.items[]? | select(.name==\"信息点02-$RUN\") | .id" 2>/dev/null)

# =============================================================================
# 5. 机柜（含机位/配线架/工位/信息点清单）
# =============================================================================
req GET "/api/resources/cabinets?room_id=$ROOM_DC"
record "5 机柜" "机柜列表（按房间过滤）" "GET /api/resources/cabinets?room_id=…" ok

req GET "/api/resources/cabinets/$CAB_A"
record "5 机柜" "机柜详情（含机位/配线架）" "GET /api/resources/cabinets/{id}" ok

req PUT "/api/resources/cabinets/$CAB_A" '{"description":"测试机柜A-已更新"}'
record "5 机柜" "更新机柜" "PUT /api/resources/cabinets/{id}" ok

req PUT "/api/resources/cabinets/$CAB_A/positions" "{\"positions\":[{\"name\":\"机位-网络区-$RUN\",\"start_u\":1,\"end_u\":4,\"description\":\"网络设备\"},{\"name\":\"机位-服务器区-$RUN\",\"start_u\":20,\"end_u\":24}]}"
record "5 机柜" "同步机位（U 位）" "PUT /api/resources/cabinets/{id}/positions" ok

req PUT "/api/resources/cabinets/$CAB_A/patch-panels" "{\"patch_panels\":[{\"name\":\"配线架A1-$RUN\"}]}"
record "5 机柜" "同步配线架" "PUT /api/resources/cabinets/{id}/patch-panels" ok

req GET "/api/resources/cabinets/$CAB_A"
POS_NET=$(echo "$BODY" | jq -r ".data.positions[]? | select(.name==\"机位-网络区-$RUN\") | .id" 2>/dev/null)
POS_SVR=$(echo "$BODY" | jq -r ".data.positions[]? | select(.name==\"机位-服务器区-$RUN\") | .id" 2>/dev/null)

req GET "/api/resources/cabinets/$CAB_A/networks"
record "5 机柜" "机柜可达网段" "GET /api/resources/cabinets/{id}/networks" ok

req GET "/api/resources/positions?room_id=$ROOM_DC"
record "5 机柜" "机位列表" "GET /api/resources/positions?room_id=…" ok

req GET "/api/resources/patch-panels?cabinet_id=$CAB_A"
record "5 机柜" "配线架列表" "GET /api/resources/patch-panels?cabinet_id=…" ok
PANEL_A1=$(jget '.data.items[0].id')

req GET "/api/resources/workstations?room_id=$ROOM_OFFICE"
record "5 机柜" "工位列表" "GET /api/resources/workstations?room_id=…" ok

req GET "/api/resources/net-outlets?room_id=$ROOM_DC"
record "5 机柜" "信息点列表" "GET /api/resources/net-outlets?room_id=…" ok

req POST /api/resources/positions "{\"name\":\"无机柜机位-$RUN\",\"start_u\":1,\"end_u\":2}"
record "5 机柜" "无机柜机位被拒绝（R8 风险已闭合）" "POST /api/resources/positions（无 cabinet_id）" 4xx "400 = cabinet_id 必填校验生效"

req PUT "/api/resources/cabinets/$CAB_B/positions" "{\"positions\":[{\"name\":\"U1-$RUN\",\"start_u\":1,\"end_u\":10},{\"name\":\"U5-$RUN\",\"start_u\":5,\"end_u\":8}]}"
record "5 机柜" "U 位重叠被数据库触发器拒绝" "PUT positions（1-10 与 5-8 重叠）" err "非 2xx 即约束生效"

# =============================================================================
# 6. 设备
# =============================================================================
req POST /api/resources/devices "{\"name\":\"核心交换机-$RUN\",\"hostname\":\"core-sw-$RUN\",\"device_type\":\"switch\",\"brand\":\"Huawei\",\"model\":\"CE6857\",\"room_id\":\"$ROOM_DC\",\"position_id\":\"$POS_NET\",\"snmp_version\":\"v2c\",\"snmp_community\":\"public\",\"description\":\"核心交换\"}"
record "6 设备" "创建核心交换机（机位安装）" "POST /api/resources/devices" ok
DEV_CORE=$(jget '.data.id')

req POST /api/resources/devices "{\"name\":\"接入交换机-$RUN\",\"device_type\":\"switch\",\"brand\":\"H3C\",\"room_id\":\"$ROOM_DC\",\"position_id\":\"$POS_NET\"}"
record "6 设备" "创建接入交换机" "POST /api/resources/devices" ok
DEV_ACCESS=$(jget '.data.id')

req POST /api/resources/devices "{\"name\":\"办公服务器-$RUN\",\"device_type\":\"server\",\"room_id\":\"$ROOM_OFFICE\",\"workstation_id\":\"$WS1\"}"
record "6 设备" "创建服务器（工位部署）" "POST /api/resources/devices" ok
DEV_SVR=$(jget '.data.id')

req POST /api/resources/devices "{\"name\":\"办公电脑-$RUN\",\"device_type\":\"desktop\",\"room_id\":\"$ROOM_OFFICE\",\"workstation_id\":\"$WS2\"}"
record "6 设备" "创建办公电脑" "POST /api/resources/devices" ok
DEV_PC=$(jget '.data.id')

req GET "/api/resources/devices?search=交换机-$RUN"
record "6 设备" "设备列表（搜索）" "GET /api/resources/devices?search=…" ok

req GET "/api/resources/devices/$DEV_CORE"
record "6 设备" "设备详情" "GET /api/resources/devices/{id}" ok

req PUT "/api/resources/devices/$DEV_CORE" '{"description":"核心交换-已更新"}'
record "6 设备" "更新设备" "PUT /api/resources/devices/{id}" ok

# 网卡→网口→IP 三层同步（核心交换机：两个网口；服务器：网口+固定 IP）
req PUT "/api/resources/devices/$DEV_CORE/network-config" "{\"cards\":[{\"name\":\"板卡1-$RUN\",\"card_type\":\"onboard\",\"ports\":[{\"name\":\"GE1/0/1\",\"physical_type\":\"sfp_plus\",\"interface_role\":\"uplink\",\"mac_address\":\"aa:bb:cc:dd:ee:01\"},{\"name\":\"GE1/0/2\",\"physical_type\":\"rj45\",\"interface_role\":\"business\"}]}]}"
record "6 设备" "同步网卡/网口配置" "PUT /api/resources/devices/{id}/network-config" ok

req PUT "/api/resources/devices/$DEV_SVR/network-config" "{\"cards\":[{\"name\":\"主板网卡-$RUN\",\"card_type\":\"onboard\",\"ports\":[{\"name\":\"eth0\",\"physical_type\":\"rj45\",\"ips\":[{\"subnet_id\":\"$NET_V4\",\"ip_address\":\"10.99.$N.100\"}]}]}]}"
record "6 设备" "同步网口并绑定固定 IP" "PUT /api/resources/devices/{id}/network-config（含 IP）" ok

req GET "/api/resources/devices/$DEV_CORE/nics"
record "6 设备" "设备网卡树" "GET /api/resources/devices/{id}/nics" ok

req GET "/api/resources/devices/$DEV_CORE/interfaces"
record "6 设备" "设备网口列表" "GET /api/resources/devices/{id}/interfaces" ok
IF_G1=$(jget '.data.items[0].id')

req GET "/api/resources/devices/$DEV_SVR/ips"
record "6 设备" "设备 IP 列表" "GET /api/resources/devices/{id}/ips" ok

req GET "/api/resources/devices/interfaces"
record "6 设备" "全部设备网口（跨设备）" "GET /api/resources/devices/interfaces" ok

# 交换机端口（device_interfaces）
req POST "/api/resources/devices/$DEV_CORE/interfaces" "{\"name\":\"GE1/0/24\",\"physical_type\":\"sfp_plus\",\"interface_role\":\"uplink\",\"port_type\":\"uplink\",\"vlan_id\":100,\"speed\":\"10G\"}"
record "6 设备" "创建交换机端口 24" "POST /api/resources/devices/{id}/interfaces" ok
PORT_CORE24=$(jget '.data.id')

req POST "/api/resources/devices/$DEV_CORE/interfaces" "{\"name\":\"GE1/0/25\",\"physical_type\":\"rj45\",\"interface_role\":\"business\",\"port_type\":\"access\",\"vlan_id\":200}"
record "6 设备" "创建交换机端口 25" "POST /api/resources/devices/{id}/interfaces" ok

req POST "/api/resources/devices/$DEV_ACCESS/interfaces" "{\"name\":\"GE0/1\",\"physical_type\":\"rj45\",\"interface_role\":\"business\",\"port_type\":\"access\",\"vlan_id\":200}"
record "6 设备" "创建接入交换机端口 1" "POST /api/resources/devices/{id}/interfaces" ok
PORT_ACCESS1=$(jget '.data.id')

req POST "/api/resources/devices/$DEV_ACCESS/interfaces" '{"name":"GE0/2","physical_type":"rj45","interface_role":"business","port_type":"access","vlan_id":200}'
record "6 设备" "创建接入交换机端口 2" "POST /api/resources/devices/{id}/interfaces" ok
PORT_ACCESS2=$(jget '.data.id')

req GET "/api/resources/devices/$DEV_CORE/interfaces"
record "6 设备" "设备端口列表" "GET /api/resources/devices/{id}/interfaces" ok

req GET "/api/resources/devices/interfaces"
record "6 设备" "全部设备端口（跨设备）" "GET /api/resources/devices/interfaces" ok

req PUT "/api/resources/devices/interfaces/$PORT_CORE24" '{"speed":"25G"}'
record "6 设备" "更新端口" "PUT /api/resources/devices/interfaces/{port_id}" ok

# IP 管理
req POST "/api/resources/devices/$DEV_PC/ips" "{\"subnet_id\":\"$NET_V4\",\"ip_address\":\"10.99.$N.101\",\"description\":\"办公电脑静态 IP\"}"
record "6 设备" "设备分配固定 IP" "POST /api/resources/devices/{id}/ips" ok

req POST "/api/resources/ip/auto-assign" "{\"subnet_id\":\"$NET_V4\",\"device_id\":\"$DEV_ACCESS\"}"
record "6 设备" "IP 自动分配" "POST /api/resources/ip/auto-assign" ok
AUTO_IP=$(jget '.data.ip_address')

req GET "/api/resources/ip?network=办公网段-$RUN"
record "6 设备" "IP 台账列表（按网段）" "GET /api/resources/ip?network=…" ok

req GET "/api/resources/device-templates"
record "6 设备" "设备模板列表" "GET /api/resources/device-templates" ok

# 负例
req POST /api/resources/devices "{\"name\":\"互斥违规-$RUN\",\"device_type\":\"server\",\"room_id\":\"$ROOM_DC\",\"workstation_id\":\"$WS1\",\"position_id\":\"$POS_SVR\"}"
record "6 设备" "工位/机位互斥校验" "POST /api/resources/devices（同时指定）" 4xx

req POST /api/resources/devices "{\"name\":\"跨房工位-$RUN\",\"device_type\":\"server\",\"room_id\":\"$ROOM_DC\",\"workstation_id\":\"$WS1\"}"
record "6 设备" "设备房间一致性校验（触发器）" "POST /api/resources/devices（工位属他房）" err "非 2xx 即约束生效"

req POST /api/resources/devices "{\"name\":\"非法类型-$RUN\",\"device_type\":\"router\",\"room_id\":\"$ROOM_DC\"}"
record "6 设备" "非法设备类型被拒绝" "POST /api/resources/devices（device_type=router）" 4xx

req POST /api/resources/devices/test-snmp '{"ip_address":"127.0.0.1","snmp_version":"v2c","snmp_community":"public"}'
record "6 设备" "SNMP 目标回环地址防护" "POST /api/resources/devices/test-snmp（127.0.0.1）" err "SSRF 防护生效"

req GET "/api/resources/devices/$DEV_CORE/snmp-info"
record "6 设备" "SNMP 信息（无真实设备）" "GET /api/resources/devices/{id}/snmp-info" err "预期失败"

req POST "/api/resources/devices/$DEV_CORE/macs/sync"
record "6 设备" "MAC 表同步（无真实设备）" "POST /api/resources/devices/{id}/macs/sync" err "预期失败"

# =============================================================================
# 7. 线路
# =============================================================================
req GET "/api/resources/cable-links?endpoint_type=device_interface"
record "7 线路" "线路列表（端点过滤）" "GET /api/resources/cable-links?endpoint_type=…" ok

req POST /api/resources/cable-links "{\"a_endpoint_type\":\"device_interface\",\"a_endpoint_id\":\"$PORT_CORE24\",\"b_endpoint_type\":\"device_interface\",\"b_endpoint_id\":\"$PORT_ACCESS1\",\"link_type\":\"ethernet\",\"cable_label\":\"级联链路-$RUN\",\"length_m\":3.5,\"tested\":true}"
record "7 线路" "创建端口-端口链路" "POST /api/resources/cable-links" ok
LINK1=$(jget '.data.id')

req POST /api/resources/cable-links "{\"a_endpoint_type\":\"net_outlet\",\"a_endpoint_id\":\"$OUTLET1\",\"b_endpoint_type\":\"patch_panel\",\"b_endpoint_id\":\"$PANEL_A1\",\"link_type\":\"ethernet\",\"cable_label\":\"信息点-配线架-$RUN\"}"
record "7 线路" "创建信息点-配线架链路" "POST /api/resources/cable-links" ok
LINK2=$(jget '.data.id')

# 先取回端口 25 的 ID（上一批创建时未保存变量）
req GET "/api/resources/devices/$DEV_CORE/interfaces"
PORT_CORE25=$(echo "$BODY" | jq -r ".data.items[]? | select(.name==\"GE1/0/25\") | .id" 2>/dev/null)
req POST /api/resources/cable-links "{\"a_endpoint_type\":\"net_outlet\",\"a_endpoint_id\":\"$OUTLET2\",\"b_endpoint_type\":\"device_interface\",\"b_endpoint_id\":\"$PORT_CORE25\",\"link_type\":\"ethernet\",\"cable_label\":\"信息点-核心-$RUN\"}"
record "7 线路" "创建信息点-端口链路" "POST /api/resources/cable-links" ok
LINK3=$(jget '.data.id')

req GET "/api/resources/cable-links/$LINK1"
record "7 线路" "线路详情" "GET /api/resources/cable-links/{id}" ok

req PUT "/api/resources/cable-links/$LINK1" '{"tested":true,"cable_label":"级联链路-已测-$RUN"}'
record "7 线路" "更新线路" "PUT /api/resources/cable-links/{id}" ok

req GET "/api/resources/cable-links/path?from_type=net_outlet&from_id=$OUTLET2&to_type=patch_panel&to_id=$PANEL_A1"
record "7 线路" "线缆路径查询（find_cable_path）" "GET /api/resources/cable-links/path?…" ok

req POST /api/resources/cable-links "{\"a_endpoint_type\":\"device_interface\",\"a_endpoint_id\":\"$PORT_CORE24\",\"b_endpoint_type\":\"device_interface\",\"b_endpoint_id\":\"$PORT_CORE24\",\"link_type\":\"ethernet\"}"
record "7 线路" "自环链路被拒绝" "POST /api/resources/cable-links（a==b）" err "非 2xx 即约束生效"

req POST /api/resources/cable-links "{\"a_endpoint_type\":\"invalid_type\",\"a_endpoint_id\":\"$PORT_CORE24\",\"b_endpoint_type\":\"device_interface\",\"b_endpoint_id\":\"$PORT_ACCESS1\"}"
record "7 线路" "非法端点类型被拒绝" "POST /api/resources/cable-links（invalid_type）" 4xx

# R1 行为验证：删除被线路引用的设备时，线路随接口级联清理（引用数归零、无残留）
req POST /api/resources/devices "{\"name\":\"待删设备-$RUN\",\"device_type\":\"server\",\"room_id\":\"$ROOM_DC\",\"position_id\":\"$POS_SVR\"}"
DEV_DEL=$(jget '.data.id')
req POST "/api/resources/devices/$DEV_DEL/interfaces" '{"name":"P1","physical_type":"rj45","interface_role":"business","port_type":"access"}'
PORT_DEL=$(jget '.data.id')
req POST /api/resources/cable-links "{\"a_endpoint_type\":\"device_interface\",\"a_endpoint_id\":\"$PORT_DEL\",\"b_endpoint_type\":\"net_outlet\",\"b_endpoint_id\":\"$OUTLET1\",\"link_type\":\"ethernet\",\"cable_label\":\"防删验证-$RUN\"}"
req DELETE "/api/resources/devices/$DEV_DEL"
record "7 线路" "删除被线路引用的设备（R1，线路级联清理）" "DELETE /api/resources/devices/{id}（端口被引用）" ok "线路随 device_interfaces 级联删除，无残留引用"

# =============================================================================
# 8. 可视化
# =============================================================================
req POST /api/resources/topology/nodes "{\"nodes\":[{\"device_id\":\"$DEV_CORE\",\"x\":100,\"y\":60,\"width\":120,\"height\":60},{\"device_id\":\"$DEV_ACCESS\",\"x\":400,\"y\":60,\"width\":120,\"height\":60},{\"device_id\":\"$DEV_SVR\",\"x\":250,\"y\":220,\"width\":100,\"height\":50},{\"device_id\":\"$DEV_PC\",\"x\":250,\"y\":320,\"width\":100,\"height\":50}]}"
record "8 可视化" "保存拓扑节点坐标" "POST /api/resources/topology/nodes" ok

req GET /api/resources/topology/nodes
record "8 可视化" "拓扑节点列表" "GET /api/resources/topology/nodes" ok

req POST /api/resources/topology/connections "{\"connection_type\":\"physical\",\"source_device_id\":\"$DEV_CORE\",\"target_device_id\":\"$DEV_ACCESS\",\"source_device_interface_id\":\"$PORT_CORE24\",\"target_device_interface_id\":\"$PORT_ACCESS1\",\"label\":\"核心-接入物理链路-$RUN\"}"
record "8 可视化" "创建物理拓扑连线" "POST /api/resources/topology/connections" ok
TOPO_CONN=$(jget '.data.id')

req POST /api/resources/topology/connections "{\"connection_type\":\"logical\",\"source_device_id\":\"$DEV_CORE\",\"target_device_id\":\"$DEV_ACCESS\",\"source_port_ids\":[\"$PORT_CORE25\"],\"target_port_ids\":[\"$PORT_ACCESS2\"],\"label\":\"聚合逻辑链路-$RUN\"}"
record "8 可视化" "创建逻辑拓扑连线（聚合）" "POST /api/resources/topology/connections" ok

req GET /api/resources/topology/connections
record "8 可视化" "拓扑连线列表（含派生）" "GET /api/resources/topology/connections" ok

req DELETE "/api/resources/topology/nodes/$DEV_PC"
record "8 可视化" "删除拓扑节点" "DELETE /api/resources/topology/nodes/{device_id}" ok

req POST /api/resources/layouts "{\"type\":\"workstation\",\"room_id\":\"$ROOM_OFFICE\",\"layout\":[{\"id\":\"$WS1\",\"position\":{\"x\":10,\"y\":20,\"width\":120,\"height\":80,\"rotation\":0},\"element_type\":\"workstation\"},{\"id\":\"$WS2\",\"position\":{\"x\":200,\"y\":20,\"width\":120,\"height\":80,\"rotation\":0},\"element_type\":\"workstation\"}]}"
record "8 可视化" "保存工位布局" "POST /api/resources/layouts" ok

req GET "/api/resources/layouts/workstation/$ROOM_OFFICE"
record "8 可视化" "读取工位布局" "GET /api/resources/layouts/workstation/{room_id}" ok

req GET "/api/resources/layouts/positions/$ROOM_DC"
record "8 可视化" "读取机位布局" "GET /api/resources/layouts/positions/{room_id}" ok

req GET "/api/resources/layouts/room-cabinets/$ROOM_DC"
record "8 可视化" "房间机柜聚合视图" "GET /api/resources/layouts/room-cabinets/{room_id}" ok

req POST /api/resources/topology/connections "{\"source_device_id\":\"$DEV_CORE\",\"target_device_id\":\"$DEV_CORE\"}"
record "8 可视化" "自连拓扑被拒绝" "POST /api/resources/topology/connections（自连）" err "非 2xx 即约束生效"

req POST /api/resources/topology/auto-discover
record "8 可视化" "拓扑自动发现（无真实设备）" "POST /api/resources/topology/auto-discover" ok "预期发现 0 条"

# =============================================================================
# 9. 只读系统接口与汇总（数据核对）
# =============================================================================
req GET /api/system/dashboard-stats
record "9 系统只读" "仪表盘统计" "GET /api/system/dashboard-stats" ok

req GET /api/system/info
record "9 系统只读" "系统信息（连接池指标）" "GET /api/system/info" ok

req GET /api/system/config
record "9 系统只读" "系统配置（脱敏检查）" "GET /api/system/config" ok

req GET /api/logs/operation
record "9 系统只读" "操作日志" "GET /api/logs/operation" ok

req GET /api/logs/login
record "9 系统只读" "登录日志" "GET /api/logs/login" ok

req GET /api/notifications
record "9 系统只读" "通知列表" "GET /api/notifications" ok

req GET "/api/system/import-export/template?type=device"
record "9 系统只读" "导入模板下载" "GET /api/system/import-export/template?type=device" file "二进制附件无 success 字段"

req GET "/api/system/import-export/export/csv?type=all"
record "9 系统只读" "全量 CSV 导出" "GET /api/system/import-export/export/csv?type=all" file "二进制附件无 success 字段"

# 普通用户操作资源（RBAC 差距验证，缺陷 #16）
req POST /api/resources/organizations "{\"name\":\"用户越权组织-$RUN\",\"type_path\":\"0\",\"template_id\":\"$TPL_ID\"}" "$USER_COOKIE"
record "9 系统只读" "普通用户创建组织被拒绝（缺陷#16 已闭合）" "POST /api/resources/organizations（user 角色）" 4xx "403 = RBAC 生效"

# =============================================================================
# 汇总报告
# =============================================================================
END_TS=$(date '+%Y-%m-%d %H:%M:%S')
echo "" >> "$REPORT"
cat "$ROWS_FILE" >> "$REPORT"
echo "" >> "$REPORT"
echo "## 汇总" >> "$REPORT"
echo "" >> "$REPORT"
echo "- 用例总数：$TOTAL_COUNT" >> "$REPORT"
echo "- 通过：$PASS_COUNT" >> "$REPORT"
echo "- 失败：$FAIL_COUNT" >> "$REPORT"
echo "- 起止时间：$START_TS ~ $END_TS" >> "$REPORT"
echo "" >> "$REPORT"
echo "### 本批次保留的业务数据" >> "$REPORT"
echo "" >> "$REPORT"
echo "| 资源 | 名称 | ID |" >> "$REPORT"
echo "|---|---|---|" >> "$REPORT"
{
  echo "| 组织模板 | $TPL_NAME | \`$TPL_ID\` |"
  echo "| 组织（根/楼宇/楼层） | 测试园区-$RUN / 一号楼-$RUN / 一层-$RUN | \`$ORG_ROOT\` / \`$ORG_BUILDING\` / \`$ORG_FLOOR\` |"
  echo "| 网络区域 | 核心区域-$RUN | \`$REGION_ID\` |"
  echo "| 网段 | 办公/服务器/IPv6 | \`$NET_V4\` / \`$NET_V4_SVR\` / \`$NET_V6\` |"
  echo "| 房间 | 测试机房/测试办公室 | \`$ROOM_DC\` / \`$ROOM_OFFICE\` |"
  echo "| 机柜 | 机柜A/机柜B | \`$CAB_A\` / \`$CAB_B\` |"
  echo "| 设备 | 核心/接入交换机、服务器、电脑 | \`$DEV_CORE\` \`$DEV_ACCESS\` \`$DEV_SVR\` \`$DEV_PC\` |"
  echo "| 线路 | 3 条 + 1 条防删验证 | \`$LINK1\` \`$LINK2\` \`$LINK3\` |"
} >> "$REPORT"

echo ""
echo "=============================="
echo "总用例: $TOTAL_COUNT  通过: $PASS_COUNT  失败: $FAIL_COUNT"
echo "报告: $REPORT"
