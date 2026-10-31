<script setup lang="ts">
/**
 * 主机监控调试弹窗：四类管理员排障工具，均手动输入目标（主机名或 IP）。
 * - Ping：有限次 ICMP 探测，回显可达性/丢包/RTT 与原始输出；
 * - SNMP：对目标一次性 GET（多 OID）或 WALK（单基 OID）取值，命中内置 MIB
 *   对照的行标注符号名；凭据手动输入（v1/v2c community，v3 USM 各字段）；
 * - MIB：OID↔名称双向静态对照；
 * - 端口/DNS：TCP 端口连通性探测、主机名 DNS 解析回显。
 * 服务端消息经 api client 就地翻译，失败走 showToast；SSRF 约束由后端统一施加。
 */
import { reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiPost } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";
import type { DnsResult } from "../../types/api/generated/DnsResult";
import type { MibEntry } from "../../types/api/generated/MibEntry";
import type { PingResult } from "../../types/api/generated/PingResult";
import type { PortResult } from "../../types/api/generated/PortResult";
import type { SnmpDebugRow } from "../../types/api/generated/SnmpDebugRow";

type DebugTab = "ping" | "snmp" | "mib" | "net";
type NetMode = "port" | "dns";

const { t } = useI18n();
const emit = defineEmits<{ close: [] }>();

const activeTab = ref<DebugTab>("ping");
const TABS: DebugTab[] = ["ping", "snmp", "mib", "net"];

function tabLabel(tab: DebugTab): string {
  return t(`debug.tab_${tab}`);
}

// ---------- Ping ----------
const ping = reactive({ host: "", count: "4", wait: "2" });
const pingBusy = ref(false);
const pingResult = ref<PingResult | null>(null);

async function runPing(): Promise<void> {
  if (!ping.host.trim()) {
    showToast(t("debug.target_required"), "warning");
    return;
  }
  pingBusy.value = true;
  try {
    const result = await apiPost<PingResult>("/api/agents/debug/ping", {
      host: ping.host.trim(),
      count: toInt(ping.count),
      timeout_secs: toInt(ping.wait)
    });
    if (result.success && result.data) {
      pingResult.value = result.data;
    } else {
      showToast(result.message || t("common.operation_failed"), "error");
    }
  } finally {
    pingBusy.value = false;
  }
}

// ---------- 端口 / DNS ----------
const net = reactive({ mode: "port" as NetMode, host: "", port: "22", timeout: "3" });
const netBusy = ref(false);
const portResult = ref<PortResult | null>(null);
const dnsResult = ref<DnsResult | null>(null);

async function runPort(): Promise<void> {
  if (!net.host.trim()) {
    showToast(t("debug.target_required"), "warning");
    return;
  }
  netBusy.value = true;
  try {
    const result = await apiPost<PortResult>("/api/agents/debug/port", {
      host: net.host.trim(),
      port: toInt(net.port) ?? 0,
      timeout_secs: toInt(net.timeout)
    });
    if (result.success && result.data) {
      portResult.value = result.data;
    } else {
      showToast(result.message || t("common.operation_failed"), "error");
    }
  } finally {
    netBusy.value = false;
  }
}

async function runDns(): Promise<void> {
  if (!net.host.trim()) {
    showToast(t("debug.target_required"), "warning");
    return;
  }
  netBusy.value = true;
  try {
    const result = await apiPost<DnsResult>("/api/agents/debug/dns", { host: net.host.trim() });
    if (result.success && result.data) {
      dnsResult.value = result.data;
    } else {
      showToast(result.message || t("common.operation_failed"), "error");
    }
  } finally {
    netBusy.value = false;
  }
}

function runNet(): Promise<void> {
  return net.mode === "port" ? runPort() : runDns();
}

// ---------- SNMP ----------
const snmp = reactive({
  target: "",
  op: "get" as "get" | "walk",
  oids: "1.3.6.1.2.1.1.1.0",
  version: "v2c",
  port: "161",
  community: "public",
  username: "",
  authProtocol: "",
  authPassword: "",
  privProtocol: "",
  privPassword: ""
});
const snmpBusy = ref(false);
const snmpRows = ref<SnmpDebugRow[] | null>(null);

async function runSnmp(): Promise<void> {
  const oids = splitOids(snmp.oids);
  if (!snmp.target.trim()) {
    showToast(t("debug.target_required"), "warning");
    return;
  }
  if (oids.length === 0) {
    showToast(t("debug.snmp.oids_required"), "warning");
    return;
  }
  snmpBusy.value = true;
  try {
    const result = await apiPost<SnmpDebugRow[]>("/api/agents/debug/snmp", {
      target: snmp.target.trim(),
      op: snmp.op,
      oids,
      snmp_version: snmp.version,
      snmp_community: blankToNull(snmp.community),
      snmp_username: blankToNull(snmp.username),
      snmp_auth_protocol: blankToNull(snmp.authProtocol),
      snmp_auth_password: blankToNull(snmp.authPassword),
      snmp_priv_protocol: blankToNull(snmp.privProtocol),
      snmp_priv_password: blankToNull(snmp.privPassword),
      snmp_port: toInt(snmp.port)
    });
    if (result.success && result.data) {
      snmpRows.value = result.data;
    } else {
      showToast(result.message || t("common.operation_failed"), "error");
    }
  } finally {
    snmpBusy.value = false;
  }
}

// ---------- MIB ----------
const mib = reactive({ query: "sysDescr" });
const mibBusy = ref(false);
const mibRows = ref<MibEntry[] | null>(null);

async function runMib(): Promise<void> {
  mibBusy.value = true;
  try {
    const result = await apiPost<MibEntry[]>("/api/agents/debug/mib", { query: mib.query.trim() });
    if (result.success && result.data) {
      mibRows.value = result.data;
      if (result.data.length === 0) {
        showToast(t("debug.mib.no_match"), "info");
      }
    } else {
      showToast(result.message || t("common.operation_failed"), "error");
    }
  } finally {
    mibBusy.value = false;
  }
}

// ---------- 小工具 ----------
/** 空白字符串转 null（未填凭据不覆盖后端缺省） */
function blankToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

/** 文本框数字转整数，空/非法返回 null */
function toInt(value: string): number | null {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

/** OID 文本按空白/逗号/分号/换行切分为数组并过滤空项 */
function splitOids(text: string): string[] {
  return text
    .split(/[\s,;]+/)
    .map((oid) => oid.trim())
    .filter((oid) => oid.length > 0);
}
</script>

<template>
  <ModalShell size="lg" @close="emit('close')">
    <template #title>{{ t("debug.title") }}</template>
    <div class="debug-body">
      <div class="debug-tabs" role="tablist" :aria-label="t('debug.title')">
        <button
          v-for="tab in TABS"
          :key="tab"
          type="button"
          class="debug-tab"
          :class="{ active: activeTab === tab }"
          role="tab"
          :aria-selected="activeTab === tab ? 'true' : 'false'"
          @click="activeTab = tab"
        >
          {{ tabLabel(tab) }}
        </button>
      </div>

      <!-- Ping -->
      <section v-if="activeTab === 'ping'" class="debug-panel">
        <div class="debug-form">
          <label class="debug-field grow">
            <span>{{ t("debug.label_target") }}</span>
            <input v-model="ping.host" type="text" :placeholder="t('debug.ph_host')" @keydown.enter="runPing">
          </label>
          <label class="debug-field narrow">
            <span>{{ t("debug.label_count") }}</span>
            <input v-model="ping.count" type="number" min="1" max="10">
          </label>
          <label class="debug-field narrow">
            <span>{{ t("debug.label_wait") }}</span>
            <input v-model="ping.wait" type="number" min="1" max="8">
          </label>
          <button type="button" class="btn btn-primary btn-sm" :disabled="pingBusy" @click="runPing">
            {{ pingBusy ? t("common.loading") : t("debug.run") }}
          </button>
        </div>
        <div v-if="pingResult" class="debug-result">
          <p class="debug-summary">
            <span class="status-badge" :class="pingResult.success ? 'status-success' : 'status-danger'">
              {{ pingResult.success ? t("debug.ping.reachable") : t("debug.ping.unreachable") }}
            </span>
            {{ pingResult.host }} ({{ pingResult.address }}) —
            {{ t("debug.ping.stat", {
              transmitted: pingResult.transmitted,
              received: pingResult.received,
              loss: pingResult.loss_pct
            }) }}
          </p>
          <p v-if="pingResult.max_ms != null" class="debug-rtt">
            RTT: {{ pingResult.min_ms }} / {{ pingResult.avg_ms }} / {{ pingResult.max_ms }} ms
          </p>
          <pre class="debug-raw">{{ pingResult.raw }}</pre>
        </div>
      </section>

      <!-- SNMP -->
      <section v-else-if="activeTab === 'snmp'" class="debug-panel">
        <div class="debug-form">
          <label class="debug-field grow">
            <span>{{ t("debug.label_target") }}</span>
            <input v-model="snmp.target" type="text" :placeholder="t('debug.ph_host')">
          </label>
          <label class="debug-field narrow">
            <span>{{ t("debug.label_op") }}</span>
            <select v-model="snmp.op">
              <option value="get">{{ t("debug.snmp.op_get") }}</option>
              <option value="walk">{{ t("debug.snmp.op_walk") }}</option>
            </select>
          </label>
          <label class="debug-field narrow">
            <span>{{ t("debug.label_port") }}</span>
            <input v-model="snmp.port" type="number" min="1" max="65535">
          </label>
        </div>
        <label class="debug-field full">
          <span>{{ snmp.op === "walk" ? t("debug.snmp.label_base") : t("debug.snmp.label_oids") }}</span>
          <textarea v-model="snmp.oids" rows="2" :placeholder="t('debug.snmp.ph_oids')"></textarea>
        </label>
        <div class="debug-form">
          <label class="debug-field narrow">
            <span>{{ t("debug.snmp.label_version") }}</span>
            <select v-model="snmp.version">
              <option value="v1">v1</option>
              <option value="v2c">v2c</option>
              <option value="v3">v3</option>
            </select>
          </label>
          <label v-if="snmp.version !== 'v3'" class="debug-field grow">
            <span>{{ t("debug.snmp.label_community") }}</span>
            <input v-model="snmp.community" type="text">
          </label>
        </div>
        <div v-if="snmp.version === 'v3'" class="debug-form v3-grid">
          <label class="debug-field">
            <span>{{ t("debug.snmp.label_username") }}</span>
            <input v-model="snmp.username" type="text">
          </label>
          <label class="debug-field">
            <span>{{ t("debug.snmp.label_auth_proto") }}</span>
            <input v-model="snmp.authProtocol" type="text" :placeholder="t('debug.snmp.ph_proto')">
          </label>
          <label class="debug-field">
            <span>{{ t("debug.snmp.label_auth_pass") }}</span>
            <input v-model="snmp.authPassword" type="password">
          </label>
          <label class="debug-field">
            <span>{{ t("debug.snmp.label_priv_proto") }}</span>
            <input v-model="snmp.privProtocol" type="text" :placeholder="t('debug.snmp.ph_proto')">
          </label>
          <label class="debug-field">
            <span>{{ t("debug.snmp.label_priv_pass") }}</span>
            <input v-model="snmp.privPassword" type="password">
          </label>
        </div>
        <button type="button" class="btn btn-primary btn-sm" :disabled="snmpBusy" @click="runSnmp">
          {{ snmpBusy ? t("common.loading") : t("debug.run") }}
        </button>

        <div v-if="snmpRows" class="debug-result">
          <table class="data-table">
            <thead>
              <tr>
                <th>{{ t("debug.snmp.col_oid") }}</th>
                <th>{{ t("debug.snmp.col_name") }}</th>
                <th>{{ t("debug.snmp.col_type") }}</th>
                <th>{{ t("debug.snmp.col_value") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-if="snmpRows.length === 0">
                <td colspan="4" class="text-center">{{ t("common.no_data") }}</td>
              </tr>
              <tr v-for="row in snmpRows" v-else :key="row.oid">
                <td class="debug-mono">{{ row.oid }}</td>
                <td>{{ row.name || "-" }}</td>
                <td>{{ row.value_type }}</td>
                <td class="debug-mono">{{ row.value }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <!-- MIB -->
      <section v-else-if="activeTab === 'mib'" class="debug-panel">
        <div class="debug-form">
          <label class="debug-field grow">
            <span>{{ t("debug.mib.label_query") }}</span>
            <input
              v-model="mib.query"
              type="text"
              :placeholder="t('debug.mib.ph_query')"
              @keydown.enter="runMib"
            >
          </label>
          <button type="button" class="btn btn-primary btn-sm" :disabled="mibBusy" @click="runMib">
            {{ mibBusy ? t("common.loading") : t("debug.query") }}
          </button>
        </div>
        <div v-if="mibRows" class="debug-result">
          <table class="data-table">
            <thead>
              <tr>
                <th>{{ t("debug.snmp.col_oid") }}</th>
                <th>{{ t("debug.snmp.col_name") }}</th>
                <th>{{ t("debug.mib.col_description") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-if="mibRows.length === 0">
                <td colspan="3" class="text-center">{{ t("debug.mib.no_match") }}</td>
              </tr>
              <tr v-for="entry in mibRows" v-else :key="entry.oid">
                <td class="debug-mono">{{ entry.oid }}</td>
                <td>{{ entry.name }}</td>
                <td>{{ entry.description }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <!-- 端口 / DNS -->
      <section v-else class="debug-panel">
        <div class="debug-form">
          <label class="debug-field narrow">
            <span>{{ t("debug.label_mode") }}</span>
            <select v-model="net.mode">
              <option value="port">{{ t("debug.net.mode_port") }}</option>
              <option value="dns">{{ t("debug.net.mode_dns") }}</option>
            </select>
          </label>
          <label class="debug-field grow">
            <span>{{ t("debug.label_target") }}</span>
            <input v-model="net.host" type="text" :placeholder="t('debug.ph_host')" @keydown.enter="runNet">
          </label>
          <label v-if="net.mode === 'port'" class="debug-field narrow">
            <span>{{ t("debug.label_port") }}</span>
            <input v-model="net.port" type="number" min="1" max="65535">
          </label>
          <label v-if="net.mode === 'port'" class="debug-field narrow">
            <span>{{ t("debug.label_timeout") }}</span>
            <input v-model="net.timeout" type="number" min="1" max="10">
          </label>
          <button type="button" class="btn btn-primary btn-sm" :disabled="netBusy" @click="runNet">
            {{ netBusy ? t("common.loading") : t("debug.run") }}
          </button>
        </div>

        <div v-if="net.mode === 'port' && portResult" class="debug-result">
          <p class="debug-summary">
            <span class="status-badge" :class="portResult.reachable ? 'status-success' : 'status-danger'">
              {{ portResult.reachable ? t("debug.net.open") : t("debug.net.closed") }}
            </span>
            {{ portResult.host }}:{{ portResult.port }} ({{ portResult.address }}) —
            {{ portResult.elapsed_ms }} ms
            <span v-if="portResult.error" class="debug-error"> · {{ portResult.error }}</span>
          </p>
        </div>
        <div v-else-if="net.mode === 'dns' && dnsResult" class="debug-result">
          <p class="debug-summary">
            {{ dnsResult.host }} — {{ dnsResult.addresses.length }} · {{ dnsResult.elapsed_ms }} ms
          </p>
          <ul class="debug-list">
            <li v-for="addr in dnsResult.addresses" :key="addr" class="debug-mono">{{ addr }}</li>
          </ul>
        </div>
      </section>
    </div>
  </ModalShell>
</template>

<style scoped>
.debug-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.debug-tabs {
  display: flex;
  gap: 6px;
  border-bottom: 1px solid var(--border-color, #ddd);
}
.debug-tab {
  padding: 6px 14px;
  border: none;
  background: transparent;
  cursor: pointer;
  font-size: 0.9rem;
  color: var(--text-muted, #666);
  border-bottom: 2px solid transparent;
}
.debug-tab.active {
  color: var(--color-primary, #2563eb);
  border-bottom-color: var(--color-primary, #2563eb);
}
.debug-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.debug-form {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: flex-end;
}
.debug-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}
.debug-field span {
  color: var(--text-muted, #888);
}
.debug-field.grow {
  flex: 1 1 220px;
}
.debug-field.narrow {
  flex: 0 0 110px;
}
.debug-field.full {
  flex: 1 1 100%;
}
.v3-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 10px;
}
.debug-field input,
.debug-field select,
.debug-field textarea {
  width: 100%;
  padding: 5px 8px;
  border: 1px solid var(--border-color, #ccc);
  border-radius: 4px;
  font-size: 13px;
}
.debug-result {
  border-top: 1px solid var(--border-color, #eee);
  padding-top: 8px;
}
.debug-summary {
  margin: 0 0 6px;
  font-size: 13px;
}
.debug-rtt {
  margin: 0 0 6px;
  font-size: 13px;
  color: var(--text-muted, #666);
}
.debug-raw {
  max-height: 160px;
  overflow: auto;
  background: var(--bg-subtle, #f6f6f6);
  padding: 8px;
  border-radius: 4px;
  font-size: 12px;
  white-space: pre-wrap;
}
.debug-mono {
  font-family: var(--font-mono, monospace);
  font-size: 12px;
}
.debug-error {
  color: var(--color-danger, #d33);
}
.debug-list {
  margin: 0;
  padding-left: 18px;
}
</style>
