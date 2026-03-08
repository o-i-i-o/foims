#!/bin/bash

# IPMA 全面API测试脚本
# 用户名: admin
# 密码: admin@123

BASE_URL="https://localhost/api"
USERNAME="admin"
PASSWORD="admin123"

# 颜色输出
GREEN="\033[0;32m"
RED="\033[0;31m"
YELLOW="\033[1;33m"
BLUE="\033[0;34m"
CYAN="\033[0;36m"
NC="\033[0m"

# 统计变量
TOTAL_TESTS=0
PASS_COUNT=0
FAIL_COUNT=0
FAIL_DETAILS=()
SKIP_COUNT=0
SKIP_DETAILS=()

# 测试数据存储
TEST_DATA_DIR="/tmp/ipma_test_data"
mkdir -p "$TEST_DATA_DIR"

# Cookie文件用于存储会话
COOKIE_FILE="$TEST_DATA_DIR/cookies.txt"
touch "$COOKIE_FILE"

# 标记是否创建了测试数据（用于清理）
CREATED_ROOM=false
CREATED_NETWORK_REGION=false

# 测试结果检查函数
check_response() {
    local response="$1"
    local test_name="$2"
    
    if echo "$response" | grep -q '"success":true'; then
        return 0
    else
        return 1
    fi
}

# 健康检查响应验证函数
check_health_response() {
    local response="$1"
    local test_name="$2"
    
    if echo "$response" | grep -q '"status":"ok"'; then
        return 0
    else
        return 1
    fi
}

# 模板响应验证函数（返回JSON数据即为成功）
check_template_response() {
    local response="$1"
    local test_name="$2"
    
    if echo "$response" | grep -q '"network_regions"'; then
        return 0
    else
        return 1
    fi
}

# 导出JSON响应验证函数（返回JSON数据即为成功）
check_export_json_response() {
    local response="$1"
    local test_name="$2"
    
    if echo "$response" | grep -q '"network_regions"'; then
        return 0
    else
        return 1
    fi
}

# 导出CSV响应验证函数（返回ZIP数据即为成功）
check_export_csv_response() {
    local response="$1"
    local test_name="$2"
    
    if echo "$response" | grep -q "PK"; then
        return 0
    else
        return 1
    fi
}

# 测试函数
run_test() {
    local test_name="$1"
    local test_command="$2"
    local expected_check="${3:-check_response}"
    local skip="${4:-false}"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ "$skip" = "true" ]; then
        SKIP_COUNT=$((SKIP_COUNT + 1))
        SKIP_DETAILS+=("$test_name")
        echo -e "${YELLOW}[SKIP] $TOTAL_TESTS. $test_name${NC}"
        return 0
    fi
    
    echo -e "${CYAN}[TEST] $TOTAL_TESTS. $test_name${NC}"
    
    local response
    response=$(eval "$test_command" 2>&1)
    local exit_code=$?
    
    if [ $exit_code -ne 0 ]; then
        FAIL_COUNT=$((FAIL_COUNT + 1))
        FAIL_DETAILS+=("$test_name: 命令执行失败 - $response")
        echo -e "${RED}[FAIL] $test_name: 命令执行失败${NC}"
        echo "       错误: $response"
        return 1
    fi
    
    if $expected_check "$response" "$test_name"; then
        PASS_COUNT=$((PASS_COUNT + 1))
        echo -e "${GREEN}[PASS] $test_name${NC}"
        echo "$response" > "$TEST_DATA_DIR/${test_name// /_}.json"
        return 0
    else
        FAIL_COUNT=$((FAIL_COUNT + 1))
        local error_msg
        error_msg=$(echo "$response" | grep -o '"message":"[^"]*' | sed 's/"message":"//' | head -1)
        FAIL_DETAILS+=("$test_name: $error_msg")
        echo -e "${RED}[FAIL] $test_name${NC}"
        echo "       响应: $response"
        return 1
    fi
}

# 提取ID的辅助函数
extract_id() {
    local response="$1"
    echo "$response" | grep -o '"id":"[^"]*' | head -1 | sed 's/"id":"//'
}

# 提取列表中第一个ID
extract_first_id() {
    local response="$1"
    echo "$response" | grep -o '"id":"[^"]*' | head -1 | sed 's/"id":"//'
}

# 打印分隔线
print_separator() {
    echo -e "${BLUE}============================================================${NC}"
}

# 打印测试组标题
print_group() {
    echo ""
    print_separator
    echo -e "${YELLOW}  $1${NC}"
    print_separator
}

echo -e "${YELLOW}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║            IPMA 全面API测试脚本                              ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"
echo "基础URL: $BASE_URL"
echo "用户名: $USERNAME"
echo "测试数据目录: $TEST_DATA_DIR"
echo ""

# ============================================
# 1. 健康检查测试
# ============================================
print_group "1. 健康检查测试"

run_test "健康检查" "curl -s -k -X GET 'https://localhost/health'" "check_health_response"

# ============================================
# 2. 认证API测试
# ============================================
print_group "2. 认证API测试"

# 登录测试 - 使用Cookie存储Token
LOGIN_RESPONSE=$(curl -s -k -c "$COOKIE_FILE" -b "$COOKIE_FILE" -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\": \"$USERNAME\", \"password\": \"$PASSWORD\", \"remember_me\": true}")

if check_response "$LOGIN_RESPONSE" "登录"; then
    echo -e "${GREEN}[PASS] 登录成功${NC}"
    PASS_COUNT=$((PASS_COUNT + 1))
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo "Cookie文件: $COOKIE_FILE"
else
    echo -e "${RED}[FAIL] 登录失败: $LOGIN_RESPONSE${NC}"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    exit 1
fi

# 获取当前用户信息
run_test "获取当前用户信息" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/auth/me' \
    -H 'Content-Type: application/json'"

# 刷新Token测试
run_test "刷新Token" "curl -s -k -b '$COOKIE_FILE' -X POST '$BASE_URL/auth/refresh'"

# ============================================
# 3. 用户管理API测试
# ============================================
print_group "3. 用户管理API测试"

# 获取用户列表
USERS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/users" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取用户列表" "echo '$USERS_RESPONSE'"

# 提取第一个用户ID用于后续测试
FIRST_USER_ID=$(extract_first_id "$USERS_RESPONSE")
if [ -n "$FIRST_USER_ID" ]; then
    run_test "获取单个用户详情" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/users/$FIRST_USER_ID' \
        -b '$COOKIE_FILE' \
        -H 'Content-Type: application/json'"
fi

# 创建测试用户
TEST_USER_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/users" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json" \
    -d '{"username": "test_user_'$(date +%s)'", "password": "Test@12345", "email": "test@example.com", "role": "user"}')

if check_response "$TEST_USER_RESPONSE" "创建测试用户"; then
    TEST_USER_ID=$(extract_id "$TEST_USER_RESPONSE")
    echo -e "${GREEN}[PASS] 创建测试用户成功, ID: $TEST_USER_ID${NC}"
    PASS_COUNT=$((PASS_COUNT + 1))
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    # 更新测试用户
    run_test "更新用户" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/users/$TEST_USER_ID' \
        -b '$COOKIE_FILE' \
        -H 'Content-Type: application/json' \
        -d '{\"email\": \"updated_test@example.com\", \"role\": \"admin\"}'"
    
    # 删除测试用户
    run_test "删除用户" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/users/$TEST_USER_ID' \
        -b '$COOKIE_FILE'"
else
    echo -e "${YELLOW}[SKIP] 创建测试用户失败，跳过相关测试${NC}"
    SKIP_COUNT=$((SKIP_COUNT + 2))
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
fi

# ============================================
# 4. 网络区域API测试
# ============================================
print_group "4. 网络区域API测试"

# 获取网络区域列表
REGIONS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/resources/network-regions" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取网络区域列表" "echo '$REGIONS_RESPONSE'"

# 创建测试网络区域
TEST_REGION_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/resources/network-regions" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json" \
    -d '{"name": "测试区域_'$(date +%s)'", "description": "API测试用网络区域"}')

if check_response "$TEST_REGION_RESPONSE" "创建网络区域"; then
    TEST_REGION_ID=$(extract_id "$TEST_REGION_RESPONSE")
    echo -e "${GREEN}[PASS] 创建网络区域成功, ID: $TEST_REGION_ID${NC}"
    PASS_COUNT=$((PASS_COUNT + 1))
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    run_test "获取单个网络区域" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/network-regions/$TEST_REGION_ID' \
        -b '$COOKIE_FILE'"
    
    run_test "更新网络区域" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/resources/network-regions/$TEST_REGION_ID' \
        -b '$COOKIE_FILE' \
        -H 'Content-Type: application/json' \
        -d '{\"name\": \"更新区域_'$(date +%s)'\", \"description\": \"更新后的描述\"}'"
else
    echo -e "${YELLOW}[SKIP] 创建网络区域失败${NC}"
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    TEST_REGION_ID=$(extract_first_id "$REGIONS_RESPONSE")
fi

# ============================================
# 5. 网络API测试
# ============================================
print_group "5. 网络API测试"

# 获取网络列表
NETWORKS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/resources/networks" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取网络列表" "echo '$NETWORKS_RESPONSE'"

# 创建测试网络（需要网络区域ID）
if [ -n "$TEST_REGION_ID" ]; then
    TEST_NETWORK_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/resources/networks" \
        -b "$COOKIE_FILE" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"测试网络_'$(date +%s)'\", \"network_region_id\": \"$TEST_REGION_ID\", \"ipv4_cidr\": \"192.168.100.0/24\", \"ipv4_gateway\": \"192.168.100.1\", \"description\": \"API测试用网络\"}")
    
    if check_response "$TEST_NETWORK_RESPONSE" "创建网络"; then
        TEST_NETWORK_ID=$(extract_id "$TEST_NETWORK_RESPONSE")
        echo -e "${GREEN}[PASS] 创建网络成功, ID: $TEST_NETWORK_ID${NC}"
        PASS_COUNT=$((PASS_COUNT + 1))
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        
        run_test "获取单个网络" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/networks/$TEST_NETWORK_ID' \
            -b '$COOKIE_FILE'"
        
        run_test "更新网络" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/resources/networks/$TEST_NETWORK_ID' \
            -b '$COOKIE_FILE' \
            -H 'Content-Type: application/json' \
            -d '{\"name\": \"更新网络_'$(date +%s)'\", \"description\": \"更新后的描述\"}'"
    else
        echo -e "${YELLOW}[SKIP] 创建网络失败${NC}"
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        TEST_NETWORK_ID=$(extract_first_id "$NETWORKS_RESPONSE")
    fi
else
    echo -e "${YELLOW}[SKIP] 无网络区域ID，跳过网络测试${NC}"
fi

# ============================================
# 6. 房间API测试
# ============================================
print_group "6. 房间API测试"

# 获取房间列表
ROOMS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/resources/rooms" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取房间列表" "echo '$ROOMS_RESPONSE'"

# 创建测试房间
TEST_ROOM_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/resources/rooms" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json" \
    -d '{"name": "测试房间_'$(date +%s)'", "room_type": "office", "network_ids": [], "description": "API测试用房间"}')

if check_response "$TEST_ROOM_RESPONSE" "创建房间"; then
    TEST_ROOM_ID=$(extract_id "$TEST_ROOM_RESPONSE")
    CREATED_ROOM=true
    echo -e "${GREEN}[PASS] 创建房间成功, ID: $TEST_ROOM_ID${NC}"
    PASS_COUNT=$((PASS_COUNT + 1))
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    run_test "获取单个房间" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/rooms/$TEST_ROOM_ID' \
        -b '$COOKIE_FILE'"
    
    run_test "获取房间网络列表" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/rooms/$TEST_ROOM_ID/networks' \
        -b '$COOKIE_FILE'"
    
    run_test "更新房间" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/resources/rooms/$TEST_ROOM_ID' \
        -b '$COOKIE_FILE' \
        -H 'Content-Type: application/json' \
        -d '{\"name\": \"更新房间_'$(date +%s)'\", \"description\": \"更新后的描述\"}'"
else
    echo -e "${YELLOW}[SKIP] 创建房间失败${NC}"
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    TEST_ROOM_ID=""
fi

# ============================================
# 7. 机柜API测试
# ============================================
print_group "7. 机柜API测试"

# 获取机柜列表
CABINETS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/resources/cabinets" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取机柜列表" "echo '$CABINETS_RESPONSE'"

# 创建测试机柜（需要房间ID）
if [ -n "$TEST_ROOM_ID" ]; then
    TEST_CABINET_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/resources/cabinets" \
        -b "$COOKIE_FILE" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"测试机柜_'$(date +%s)'\", \"room_id\": \"$TEST_ROOM_ID\", \"capacity\": 42, \"network_ids\": [], \"description\": \"API测试用机柜\"}")
    
    if check_response "$TEST_CABINET_RESPONSE" "创建机柜"; then
        TEST_CABINET_ID=$(extract_id "$TEST_CABINET_RESPONSE")
        echo -e "${GREEN}[PASS] 创建机柜成功, ID: $TEST_CABINET_ID${NC}"
        PASS_COUNT=$((PASS_COUNT + 1))
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        
        run_test "获取单个机柜" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/cabinets/$TEST_CABINET_ID' \
            -b '$COOKIE_FILE'"
        
        run_test "获取机柜网络列表" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/cabinets/$TEST_CABINET_ID/networks' \
            -b '$COOKIE_FILE'"
        
        run_test "更新机柜" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/resources/cabinets/$TEST_CABINET_ID' \
            -b '$COOKIE_FILE' \
            -H 'Content-Type: application/json' \
            -d '{\"name\": \"更新机柜_'$(date +%s)'\", \"description\": \"更新后的描述\"}'"
    else
        echo -e "${YELLOW}[SKIP] 创建机柜失败${NC}"
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        TEST_CABINET_ID=$(extract_first_id "$CABINETS_RESPONSE")
    fi
else
    echo -e "${YELLOW}[SKIP] 无房间ID，跳过机柜测试${NC}"
fi

# ============================================
# 8. 工位API测试
# ============================================
print_group "8. 工位API测试"

# 获取工位列表
WORKSTATIONS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/resources/workstations" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取工位列表" "echo '$WORKSTATIONS_RESPONSE'"

# 创建测试工位
if [ -n "$TEST_ROOM_ID" ]; then
    TEST_WORKSTATION_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/resources/workstations" \
        -b "$COOKIE_FILE" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"测试工位_'$(date +%s)'\", \"room_id\": \"$TEST_ROOM_ID\", \"manager\": \"测试管理员\", \"description\": \"API测试用工位\"}")
    
    if check_response "$TEST_WORKSTATION_RESPONSE" "创建工位"; then
        TEST_WORKSTATION_ID=$(extract_id "$TEST_WORKSTATION_RESPONSE")
        echo -e "${GREEN}[PASS] 创建工位成功, ID: $TEST_WORKSTATION_ID${NC}"
        PASS_COUNT=$((PASS_COUNT + 1))
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        
        run_test "获取单个工位" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/workstations/$TEST_WORKSTATION_ID' \
            -b '$COOKIE_FILE'"
        
        run_test "更新工位" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/resources/workstations/$TEST_WORKSTATION_ID' \
            -b '$COOKIE_FILE' \
            -H 'Content-Type: application/json' \
            -d '{\"name\": \"更新工位_'$(date +%s)'\", \"description\": \"更新后的描述\"}'"
    else
        echo -e "${YELLOW}[SKIP] 创建工位失败${NC}"
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        TEST_WORKSTATION_ID=$(extract_first_id "$WORKSTATIONS_RESPONSE")
    fi
else
    echo -e "${YELLOW}[SKIP] 无房间ID，跳过工位测试${NC}"
fi

# ============================================
# 9. 机位API测试
# ============================================
print_group "9. 机位API测试"

# 获取机位列表
POSITIONS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/resources/positions" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取机位列表" "echo '$POSITIONS_RESPONSE'"

# 创建测试机位（需要机柜ID）
if [ -n "$TEST_CABINET_ID" ]; then
    TEST_POSITION_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/resources/positions" \
        -b "$COOKIE_FILE" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"测试机位_'$(date +%s)'\", \"cabinet_id\": \"$TEST_CABINET_ID\", \"start_u\": 1, \"end_u\": 2, \"description\": \"API测试用机位\"}")
    
    if check_response "$TEST_POSITION_RESPONSE" "创建机位"; then
        TEST_POSITION_ID=$(extract_id "$TEST_POSITION_RESPONSE")
        echo -e "${GREEN}[PASS] 创建机位成功, ID: $TEST_POSITION_ID${NC}"
        PASS_COUNT=$((PASS_COUNT + 1))
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        
        run_test "获取单个机位" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/positions/$TEST_POSITION_ID' \
            -b '$COOKIE_FILE'"
        
        run_test "更新机位" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/resources/positions/$TEST_POSITION_ID' \
            -b '$COOKIE_FILE' \
            -H 'Content-Type: application/json' \
            -d '{\"name\": \"更新机位_'$(date +%s)'\", \"start_u\": 3, \"end_u\": 4}'"
    else
        echo -e "${YELLOW}[SKIP] 创建机位失败${NC}"
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        TEST_POSITION_ID=$(extract_first_id "$POSITIONS_RESPONSE")
    fi
else
    echo -e "${YELLOW}[SKIP] 无机柜ID，跳过机位测试${NC}"
fi

# ============================================
# 10. IP管理API测试
# ============================================
print_group "10. IP管理API测试"

# 获取IP管理列表
IP_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/resources/ip" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取IP管理列表" "echo '$IP_RESPONSE'"

# 获取可用IP（需要网络ID）
if [ -n "$TEST_NETWORK_ID" ]; then
    run_test "获取可用IP列表" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/ip/available/$TEST_NETWORK_ID' \
        -b '$COOKIE_FILE'"
fi

# 获取工位IP
if [ -n "$TEST_WORKSTATION_ID" ]; then
    run_test "获取工位IP列表" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/ip/workstation/$TEST_WORKSTATION_ID' \
        -b '$COOKIE_FILE'"
fi

# 获取机位IP
if [ -n "$TEST_POSITION_ID" ]; then
    run_test "获取机位IP列表" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/ip/cabinet-position/$TEST_POSITION_ID' \
        -b '$COOKIE_FILE'"
fi

# ============================================
# 11. 交换机API测试
# ============================================
print_group "11. 交换机API测试"

# 获取交换机列表
SWITCHES_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/switches" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取交换机列表" "echo '$SWITCHES_RESPONSE'"

# 创建测试交换机
if [ -n "$TEST_REGION_ID" ] && [ -n "$TEST_NETWORK_ID" ]; then
    TEST_SWITCH_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/switches" \
        -b "$COOKIE_FILE" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"测试交换机_'$(date +%s)'\", \"network_region_id\": \"$TEST_REGION_ID\", \"network_id\": \"$TEST_NETWORK_ID\", \"snmp_version\": \"v2c\", \"snmp_community\": \"public\", \"description\": \"API测试用交换机\", \"ips\": [{\"network_id\": \"$TEST_NETWORK_ID\", \"ip_address\": \"192.168.100.100\"}]}")
    
    if check_response "$TEST_SWITCH_RESPONSE" "创建交换机"; then
        TEST_SWITCH_ID=$(extract_id "$TEST_SWITCH_RESPONSE")
        echo -e "${GREEN}[PASS] 创建交换机成功, ID: $TEST_SWITCH_ID${NC}"
        PASS_COUNT=$((PASS_COUNT + 1))
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        
        run_test "获取单个交换机" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/switches/$TEST_SWITCH_ID' \
            -b '$COOKIE_FILE'"
        
        run_test "更新交换机" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/switches/$TEST_SWITCH_ID' \
            -b '$COOKIE_FILE' \
            -H 'Content-Type: application/json' \
            -d '{\"name\": \"更新交换机_'$(date +%s)'\", \"description\": \"更新后的描述\"}'"
        
        run_test "获取交换机端口列表" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/switches/$TEST_SWITCH_ID/ports' \
            -b '$COOKIE_FILE'"
        
        run_test "测试SNMP连接" "curl -s -k -b '$COOKIE_FILE' -X POST '$BASE_URL/switches/$TEST_SWITCH_ID/test-snmp' \
            -b '$COOKIE_FILE' \
            -H 'Content-Type: application/json' \
            -d '{}'"
        
        run_test "获取交换机ARP表" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/switches/$TEST_SWITCH_ID/arp-table' \
            -b '$COOKIE_FILE'"
        
        run_test "获取交换机SNMP信息" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/switches/$TEST_SWITCH_ID/snmp-info' \
            -b '$COOKIE_FILE'"
        
        run_test "获取交换机SNMP端口" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/switches/$TEST_SWITCH_ID/snmp-ports' \
            -b '$COOKIE_FILE'"
    else
        echo -e "${YELLOW}[SKIP] 创建交换机失败${NC}"
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        TEST_SWITCH_ID=$(extract_first_id "$SWITCHES_RESPONSE")
    fi
else
    echo -e "${YELLOW}[SKIP] 无网络区域ID，跳过交换机测试${NC}"
fi

# ============================================
# 12. 交换机端口API测试
# ============================================
print_group "12. 交换机端口API测试"

# 获取所有交换机端口
run_test "获取所有交换机端口" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/switches/ports' \
    -b '$COOKIE_FILE'"

# 创建测试端口（需要交换机ID）
if [ -n "$TEST_SWITCH_ID" ]; then
    TEST_PORT_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X POST "$BASE_URL/switches/$TEST_SWITCH_ID/ports" \
        -b "$COOKIE_FILE" \
        -H "Content-Type: application/json" \
        -d '{"port_number": "Gi0/1", "port_name": "测试端口", "port_type": "gigabit", "status": "up"}')
    
    if check_response "$TEST_PORT_RESPONSE" "创建交换机端口"; then
        TEST_PORT_ID=$(extract_id "$TEST_PORT_RESPONSE")
        echo -e "${GREEN}[PASS] 创建交换机端口成功, ID: $TEST_PORT_ID${NC}"
        PASS_COUNT=$((PASS_COUNT + 1))
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        
        run_test "获取单个交换机端口" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/switches/ports/$TEST_PORT_ID' \
            -b '$COOKIE_FILE'"
        
        run_test "更新交换机端口" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/switches/ports/$TEST_PORT_ID' \
            -b '$COOKIE_FILE' \
            -H 'Content-Type: application/json' \
            -d '{\"port_name\": \"更新端口\", \"status\": \"down\"}'"
        
        # 删除测试端口
        run_test "删除交换机端口" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/switches/ports/$TEST_PORT_ID' \
            -b '$COOKIE_FILE'"
    else
        echo -e "${YELLOW}[SKIP] 创建交换机端口失败${NC}"
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
    fi
fi

# ============================================
# 13. 日志API测试
# ============================================
print_group "13. 日志API测试"

run_test "获取操作日志" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/logs/operation' \
    -b '$COOKIE_FILE'"

run_test "获取登录日志" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/logs/login' \
    -b '$COOKIE_FILE'"

# ============================================
# 14. 通知API测试
# ============================================
print_group "14. 通知API测试"

NOTIFICATIONS_RESPONSE=$(curl -s -k -b "$COOKIE_FILE" -X GET "$BASE_URL/notifications" \
    -b "$COOKIE_FILE" \
    -H "Content-Type: application/json")

run_test "获取通知列表" "echo '$NOTIFICATIONS_RESPONSE'"

# 提取第一个通知ID
FIRST_NOTIFICATION_ID=$(extract_first_id "$NOTIFICATIONS_RESPONSE")
if [ -n "$FIRST_NOTIFICATION_ID" ]; then
    run_test "标记单个通知已读" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/notifications/$FIRST_NOTIFICATION_ID/read' \
        -b '$COOKIE_FILE'"
fi

run_test "标记所有通知已读" "curl -s -k -b '$COOKIE_FILE' -X PUT '$BASE_URL/notifications/mark-all-read' \
    -b '$COOKIE_FILE'"

# ============================================
# 15. 系统管理API测试
# ============================================
print_group "15. 系统管理API测试"

run_test "获取系统信息" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/info' \
    -b '$COOKIE_FILE'"

run_test "获取仪表盘统计" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/dashboard-stats' \
    -b '$COOKIE_FILE'"

run_test "获取服务状态" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/service-status' \
    -b '$COOKIE_FILE'"

run_test "获取SMTP配置" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/smtp/config' \
    -b '$COOKIE_FILE'"

run_test "获取证书状态" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/certificate/status' \
    -b '$COOKIE_FILE'"

run_test "获取支持的语言" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/languages' \
    -b '$COOKIE_FILE'"

run_test "获取页面超时配置" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/page-timeout' \
    -b '$COOKIE_FILE'"

run_test "获取会话超时配置" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/session-timeout' \
    -b '$COOKIE_FILE'"

# ============================================
# 16. 导入导出API测试
# ============================================
print_group "16. 导入导出API测试"

run_test "导出JSON" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/import-export/export/json'" "check_export_json_response"

run_test "导出CSV" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/import-export/export/csv'" "check_export_csv_response"

run_test "下载模板" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/system/import-export/template'" "check_template_response"

# ============================================
# 17. 布局API测试
# ============================================
print_group "17. 布局API测试"

if [ -n "$TEST_ROOM_ID" ]; then
    run_test "获取房间布局" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/layouts/workstation/$TEST_ROOM_ID' \
        -b '$COOKIE_FILE'"
fi

if [ -n "$TEST_REGION_ID" ]; then
    run_test "获取网络区域布局" "curl -s -k -b '$COOKIE_FILE' -X GET '$BASE_URL/resources/layouts/positions/$TEST_REGION_ID' \
        -b '$COOKIE_FILE'"
fi

# ============================================
# 18. 清理测试数据
# ============================================
print_group "18. 清理测试数据"

# 按依赖关系逆序删除
if [ -n "$TEST_POSITION_ID" ]; then
    run_test "删除测试机位" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/resources/positions/$TEST_POSITION_ID' \
        -b '$COOKIE_FILE'"
fi

if [ -n "$TEST_WORKSTATION_ID" ]; then
    run_test "删除测试工位" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/resources/workstations/$TEST_WORKSTATION_ID' \
        -b '$COOKIE_FILE'"
fi

if [ -n "$TEST_CABINET_ID" ]; then
    run_test "删除测试机柜" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/resources/cabinets/$TEST_CABINET_ID' \
        -b '$COOKIE_FILE'"
fi

if [ -n "$TEST_SWITCH_ID" ]; then
    run_test "删除测试交换机" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/switches/$TEST_SWITCH_ID' \
        -b '$COOKIE_FILE'"
fi

if [ -n "$TEST_NETWORK_ID" ]; then
    run_test "删除测试网络" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/resources/networks/$TEST_NETWORK_ID' \
        -b '$COOKIE_FILE'"
fi

if [ "$CREATED_ROOM" = true ] && [ -n "$TEST_ROOM_ID" ]; then
    run_test "删除测试房间" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/resources/rooms/$TEST_ROOM_ID' \
        -b '$COOKIE_FILE'"
fi

if [ -n "$TEST_REGION_ID" ]; then
    run_test "删除测试网络区域" "curl -s -k -b '$COOKIE_FILE' -X DELETE '$BASE_URL/resources/network-regions/$TEST_REGION_ID' \
        -b '$COOKIE_FILE'"
fi

# ============================================
# 19. 登出测试
# ============================================
print_group "19. 登出测试"

run_test "登出" "curl -s -k -b '$COOKIE_FILE' -X POST '$BASE_URL/auth/logout' \
    -b '$COOKIE_FILE' \
    -H 'Content-Type: application/json'"

# ============================================
# 测试结果汇总
# ============================================
print_group "测试结果汇总"

echo ""
echo -e "${CYAN}总测试数: $TOTAL_TESTS${NC}"
echo -e "${GREEN}通过: $PASS_COUNT${NC}"
echo -e "${RED}失败: $FAIL_COUNT${NC}"
echo -e "${YELLOW}跳过: $SKIP_COUNT${NC}"
echo ""

# 计算通过率
if [ $TOTAL_TESTS -gt 0 ]; then
    PASS_RATE=$(echo "scale=2; $PASS_COUNT * 100 / $TOTAL_TESTS" | bc)
    echo "通过率: ${PASS_RATE}%"
fi

# 显示失败详情
if [ ${#FAIL_DETAILS[@]} -gt 0 ]; then
    echo ""
    echo -e "${RED}失败详情:${NC}"
    for detail in "${FAIL_DETAILS[@]}"; do
        echo "  - $detail"
    done
fi

# 显示跳过详情
if [ ${#SKIP_DETAILS[@]} -gt 0 ]; then
    echo ""
    echo -e "${YELLOW}跳过的测试:${NC}"
    for detail in "${SKIP_DETAILS[@]}"; do
        echo "  - $detail"
    done
fi

echo ""
print_separator

# 清理测试数据目录
rm -rf "$TEST_DATA_DIR"

# 返回退出码
if [ $FAIL_COUNT -gt 0 ]; then
    echo -e "${RED}测试完成，存在失败的测试用例${NC}"
    exit 1
else
    echo -e "${GREEN}所有测试通过!${NC}"
    exit 0
fi
