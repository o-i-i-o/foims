<script setup lang="ts">
/**
 * SNMP Trap 接收配置卡片（移植自旧版 systemManager.js 的 trap 段）：
 * 启用开关 / 监听地址与端口 / 冷却窗口 / 团体名白名单 /
 * v3 USM 用户表（协议下拉 + 密码掩码 *** 沿用规则 + 逐行校验）。
 * PUT /api/system/config 会整体替换 snmp 段，基础参数原样带回避免丢配置。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { getSystemConfig, updateSystemConfig } from "../../api/system";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";

/** GET 配置中的 trap 段 USM 用户行（历史数据字段可能缺省） */
interface TrapUserConfig {
  username?: string | null;
  auth_protocol?: string | null;
  auth_password?: string | null;
  priv_protocol?: string | null;
  priv_password?: string | null;
}

/** trap 段配置（GET /api/system/config 的 snmp.trap 段，后端未生成 ts-rs 类型） */
interface TrapSection {
  enabled?: boolean;
  /** 监听地址主机部分（IPv4/主机名/方括号 IPv6），端口见 port */
  bind_addr?: string | null;
  port?: number | null;
  communities?: string[] | null;
  users?: TrapUserConfig[] | null;
  cooldown_secs?: number | null;
}

/** snmp 段配置（本卡片仅消费基础参数 + trap 段） */
interface SnmpConfigSection {
  timeout_secs?: number;
  retries?: number;
  lldp_timeout_secs?: number;
  mac_scan_timeout_secs?: number;
  trap?: TrapSection | null;
}

/** GET/PUT /api/system/config 局部载荷（本卡片仅带 snmp 段） */
interface SnmpConfigPayload {
  snmp?: SnmpConfigSection | null;
}

/** PUT 提交的 trap USM 用户行 */
interface TrapUserPayload {
  username: string;
  auth_protocol: string;
  auth_password: string;
  priv_protocol: string;
  priv_password: string;
}

/** USM 用户表单行：hasValue 标记后端已配置密码（回传 ***），输入框保持留空 */
interface TrapUserRow {
  username: string;
  auth_protocol: string;
  auth_password: string;
  authHasValue: boolean;
  priv_protocol: string;
  priv_password: string;
  privHasValue: boolean;
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

// 与后端一致的密码掩码：*** 表示沿用磁盘上已保存的密码
const SNMP_TRAP_PASSWORD_MASK = "***";
// 后端 trap.rs 接受的协议取值（认证 / 加密），与设备 SNMP 弹窗的取值集不同
const AUTH_PROTOCOLS = ["md5", "sha", "sha224", "sha256", "sha384", "sha512"];
const PRIV_PROTOCOLS = ["des", "3des", "aes128", "aes192", "aes256"];

const configQuery = useQuery({
  queryKey: ["system-config"],
  queryFn: async () => {
    const result = await getSystemConfig<SnmpConfigPayload>();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

// 最近一次加载的 SNMP 基础参数（超时/重试等与 trap 无关的字段），保存时原样带回
let snmpBase: Pick<SnmpConfigSection, "timeout_secs" | "retries" | "lldp_timeout_secs" | "mac_scan_timeout_secs"> = {};

const trapEnabled = ref(false);
const bindHost = ref("0.0.0.0");
const bindPort = ref("162");
const cooldown = ref("30");
const communities = ref("");
const trapUsers = ref<TrapUserRow[]>([]);

// 存量协议值大小写归一（后端解析大小写不敏感）；无法识别的取值回落为不启用
function normalizeProtocol(value: string | null | undefined, protocols: string[]): string {
  const v = (value || "").toLowerCase();
  return protocols.includes(v) ? v : "";
}

function buildRow(user: TrapUserConfig | null): TrapUserRow {
  return {
    username: user?.username || "",
    auth_protocol: normalizeProtocol(user?.auth_protocol, AUTH_PROTOCOLS),
    auth_password: "",
    authHasValue: user?.auth_password === SNMP_TRAP_PASSWORD_MASK,
    priv_protocol: normalizeProtocol(user?.priv_protocol, PRIV_PROTOCOLS),
    priv_password: "",
    privHasValue: user?.priv_password === SNMP_TRAP_PASSWORD_MASK
  };
}

watch(
  () => configQuery.data.value,
  (data) => {
    if (!data?.snmp) {
      return;
    }
    const snmp = data.snmp;
    snmpBase = {
      timeout_secs: snmp.timeout_secs ?? 5,
      retries: snmp.retries ?? 3,
      lldp_timeout_secs: snmp.lldp_timeout_secs ?? 30,
      mac_scan_timeout_secs: snmp.mac_scan_timeout_secs ?? 10
    };
    const trap = snmp.trap || {};
    trapEnabled.value = Boolean(trap.enabled);
    bindHost.value = trap.bind_addr || "0.0.0.0";
    bindPort.value = String(trap.port ?? 162);
    cooldown.value = String(trap.cooldown_secs ?? 30);
    communities.value = (trap.communities || []).join(", ");
    trapUsers.value = (trap.users || []).map((user) => buildRow(user));
  }
);

function addUser(): void {
  trapUsers.value.push(buildRow(null));
}

function removeRow(index: number): void {
  trapUsers.value.splice(index, 1);
}

// 读取单个密码框的提交值：留空且有已配置标记时回传掩码由后端沿用磁盘值；
// 配置了协议时密码必填，且新输入须满足 RFC 3414 的 8 字符下限。校验失败返回 null
function readPassword(
  row: TrapUserRow,
  field: "auth_password" | "priv_password",
  hasField: "authHasValue" | "privHasValue",
  protocol: string
): string | null {
  const value = row[field];
  if (value !== "") {
    if (protocol && value.length < 8) {
      showToast(t("snmp_trap.password_too_short", { username: row.username }), "warning");
      return null;
    }
    return value;
  }
  if (row[hasField]) {
    return SNMP_TRAP_PASSWORD_MASK;
  }
  if (protocol) {
    showToast(t("snmp_trap.password_required", { username: row.username }), "warning");
    return null;
  }
  return "";
}

// 收集并校验用户表；校验失败提示后返回 null
function collectUsers(): TrapUserPayload[] | null {
  const users: TrapUserPayload[] = [];
  const seenUsernames = new Set<string>();
  for (const row of trapUsers.value) {
    const username = row.username.trim();
    // USM 无 noAuth + priv 组合：加密必须与认证配套（与后端校验一致）
    if (!username) {
      showToast(t("snmp_trap.username_required"), "warning");
      return null;
    }
    if (seenUsernames.has(username)) {
      showToast(t("snmp_trap.duplicate_username", { username }), "warning");
      return null;
    }
    seenUsernames.add(username);
    if (row.priv_protocol && !row.auth_protocol) {
      showToast(t("snmp_trap.priv_requires_auth", { username }), "warning");
      return null;
    }
    const authPassword = readPassword(row, "auth_password", "authHasValue", row.auth_protocol);
    if (authPassword === null) {
      return null;
    }
    const privPassword = readPassword(row, "priv_password", "privHasValue", row.priv_protocol);
    if (privPassword === null) {
      return null;
    }
    users.push({
      username,
      auth_protocol: row.auth_protocol,
      auth_password: authPassword,
      priv_protocol: row.priv_protocol,
      priv_password: privPassword
    });
  }
  return users;
}

const saving = ref(false);

async function saveSnmpTrapConfig(): Promise<void> {
  if (saving.value) {
    return;
  }
  // 地址为 IPv4/主机名或方括号 IPv6；端口 1-65535
  const host = bindHost.value.trim();
  const port = Number.parseInt(bindPort.value, 10);
  const hostValid = /^(?:\[[0-9a-fA-F:]+\]|[0-9A-Za-z.-]+)$/.test(host);
  const portValid = Number.isInteger(port) && port >= 1 && port <= 65535;
  if (!hostValid || !portValid) {
    showToast(t("snmp_trap.invalid_bind_addr"), "warning");
    return;
  }

  const cooldownSecs = Math.min(Math.max(Number.parseInt(cooldown.value, 10) || 0, 0), 86400);
  const communityList = communities.value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
  const users = collectUsers();
  if (users === null) {
    return;
  }

  saving.value = true;
  try {
    const payload: { snmp: SnmpConfigSection } = {
      snmp: {
        ...snmpBase,
        trap: {
          enabled: trapEnabled.value,
          bind_addr: host,
          port,
          communities: communityList,
          users,
          cooldown_secs: cooldownSecs
        }
      }
    };
    const result = await updateSystemConfig(payload);
    if (result.success) {
      showToast(t("snmp_trap.save_success"), "success");
      showToast(t("system.config_saved_restart_needed"), "warning");
      // 重新加载以刷新密码占位符（后端对新密码返回 ***）
      queryClient.invalidateQueries({ queryKey: ["system-config"] });
    } else {
      showToast(`${t("snmp_trap.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <section class="config-card">
    <header class="config-card-header">
      <div class="config-card-title">
        <span class="icon">📡</span>
        <span>{{ t("snmp_trap.title") }}</span>
      </div>
      <div v-if="!readonly" class="config-switch">
        <label>
          <input v-model="trapEnabled" type="checkbox">
          <span>{{ t("snmp_trap.enabled") }}</span>
        </label>
        <button type="button" class="btn btn-primary btn-sm" :disabled="saving" @click="saveSnmpTrapConfig">
          {{ t("common.save") }}
        </button>
      </div>
    </header>
    <div class="form-row">
      <div class="form-group">
        <label for="snmp-trap-bind-addr">{{ t("snmp_trap.bind_addr") }}</label>
        <input id="snmp-trap-bind-addr" v-model="bindHost" type="text" placeholder="0.0.0.0" autocomplete="off">
      </div>
      <div class="form-group">
        <label for="snmp-trap-bind-port">{{ t("snmp_trap.bind_port") }}</label>
        <input id="snmp-trap-bind-port" v-model="bindPort" type="number" min="1" max="65535" placeholder="162">
      </div>
      <div class="form-group">
        <label for="snmp-trap-cooldown">{{ t("snmp_trap.cooldown_secs") }}</label>
        <input id="snmp-trap-cooldown" v-model="cooldown" type="number" min="0" max="86400" placeholder="30">
      </div>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="snmp-trap-communities">{{ t("snmp_trap.communities") }}</label>
        <input id="snmp-trap-communities" v-model="communities" type="text" :placeholder="t('snmp_trap.communities_placeholder')" autocomplete="off">
      </div>
    </div>
    <div class="form-row">
      <div class="form-group form-group-block">
        <div class="group-label-row">
          <span class="group-label">{{ t("snmp_trap.users") }}</span>
          <button v-if="!readonly" type="button" class="btn btn-secondary btn-sm" @click="addUser">
            {{ t("snmp_trap.add_user") }}
          </button>
        </div>
        <div class="table-container snmp-trap-users-container">
          <table class="data-table">
            <thead>
              <tr>
                <th>{{ t("snmp_trap.username") }}</th>
                <th>{{ t("snmp_trap.auth_protocol") }}</th>
                <th>{{ t("snmp_trap.auth_password") }}</th>
                <th>{{ t("snmp_trap.priv_protocol") }}</th>
                <th>{{ t("snmp_trap.priv_password") }}</th>
                <th v-if="!readonly"></th>
              </tr>
            </thead>
            <tbody>
              <tr v-if="trapUsers.length === 0" class="empty-row">
                <td class="text-center" :colspan="readonly ? 5 : 6">{{ t("snmp_trap.no_users") }}</td>
              </tr>
              <tr v-for="(row, index) in trapUsers" v-else :key="index">
                <td><input v-model="row.username" type="text" :placeholder="t('snmp_trap.username')" autocomplete="off"></td>
                <td>
                  <select v-model="row.auth_protocol">
                    <option value="">{{ t("snmp_trap.no_auth") }}</option>
                    <option v-for="protocol in AUTH_PROTOCOLS" :key="protocol" :value="protocol">{{ protocol.toUpperCase() }}</option>
                  </select>
                </td>
                <td>
                  <input
                    v-model="row.auth_password"
                    type="password"
                    autocomplete="new-password"
                    :placeholder="t(row.authHasValue ? 'snmp_trap.password_configured' : 'snmp_trap.password_placeholder')"
                  >
                </td>
                <td>
                  <select v-model="row.priv_protocol">
                    <option value="">{{ t("snmp_trap.no_priv") }}</option>
                    <option v-for="protocol in PRIV_PROTOCOLS" :key="protocol" :value="protocol">{{ protocol.toUpperCase() }}</option>
                  </select>
                </td>
                <td>
                  <input
                    v-model="row.priv_password"
                    type="password"
                    autocomplete="new-password"
                    :placeholder="t(row.privHasValue ? 'snmp_trap.password_configured' : 'snmp_trap.password_placeholder')"
                  >
                </td>
                <td v-if="!readonly">
                  <button type="button" class="btn btn-danger btn-sm" @click="removeRow(index)">
                    {{ t("common.delete") }}
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  </section>
</template>
