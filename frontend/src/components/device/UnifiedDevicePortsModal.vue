<script setup lang="ts">
/**
 * 统一设备端口管理弹窗：
 * - 数据源 GET /devices/{id}/nics（网卡 → 网口），每张网卡一组端口方块，
 *   组内按 sort_order 与名称排序；托管与非托管网口统一展示，点击编辑；
 * - 端口方块显示名称末尾数字段，tooltip 汇总名称/MAC/IP/网段/VLAN/类型/
 *   状态（非 up）/速率；
 * - 底部：添加端口（默认不托管）/ 从 SNMP 获取端口；
 * - SNMP 同步：GET /snmp-ports 与现有网口按名称（忽略大小写）比对，
 *   冲突时弹 PortConflictModal 逐项决策，新增端口 device_managed=false，
 *   覆盖仅更新运行属性；任务按顺序逐个提交，结果汇总提示。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";
import PortConflictModal from "./PortConflictModal.vue";
import PortDetailModal from "./PortDetailModal.vue";
import type { DeviceInterface } from "../../types/api/generated/DeviceInterface";
import type { DeviceInterfaceCreate } from "../../types/api/generated/DeviceInterfaceCreate";
import type { DeviceInterfaceUpdate } from "../../types/api/generated/DeviceInterfaceUpdate";
import type { IpDetail } from "../../types/api/generated/IpDetail";
import type { NetworkCard } from "../../types/api/generated/NetworkCard";
import type { SnmpPort } from "../../types/api/generated/SnmpPort";

/** GET /devices/{id}/nics 的网口行（含 IP 明细） */
type NicPort = DeviceInterface & { ips?: IpDetail[] };
/** GET /devices/{id}/nics 的网卡行 */
type NicCard = NetworkCard & { ports?: NicPort[] };

const props = withDefaults(
  defineProps<{
    deviceId: string;
    deviceName?: string;
  }>(),
  { deviceName: "" }
);

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 端口数据 ====================

const nicsQuery = useQuery({
  queryKey: computed(() => ["device-nics", props.deviceId]),
  queryFn: async (): Promise<NicCard[]> => {
    const result = await apiGet<{ cards?: NicCard[] }>(
      `/api/resources/devices/${props.deviceId}/nics`
    );
    if (!result.success) {
      throw new Error(result.message || t("device.load_failed"));
    }
    return result.data?.cards || [];
  }
});

const cards = computed(() => nicsQuery.data.value ?? []);

/** 展示端口名中的末尾数字段；不含数字显示全名，空名显示 ? */
function extractPortLastNumber(portName: string): string {
  if (typeof portName !== "string" || !portName) {
    return portName || "";
  }
  const match = portName.match(/\d+/g);
  if (match) {
    // /\d+/g 的匹配组非空，at(-1) 必有值；?? 仅用于类型收窄
    return match.at(-1) ?? portName;
  }
  return portName;
}

/** 组内端口排序：sort_order 优先，其次名称 */
function sortedPorts(card: NicCard): NicPort[] {
  const ports = Array.isArray(card.ports) ? [...card.ports] : [];
  ports.sort(
    (a, b) =>
      (a.sort_order ?? 0) - (b.sort_order ?? 0) || String(a.name).localeCompare(String(b.name))
  );
  return ports;
}

/** 端口方块 tooltip：名称 | MAC | IP(网段/区域) | VLAN | 类型 | 状态 | 速率 */
function portTooltip(iface: NicPort): string {
  const ipList = Array.isArray(iface.ips)
    ? iface.ips
        .map((i) => {
          const netInfo = [i.network_region, i.network_name].filter(Boolean).join(" / ");
          return netInfo ? `${i.ip_address} (${netInfo})` : i.ip_address;
        })
        .filter(Boolean)
    : [];
  const parts = [iface.name || ""];
  if (iface.mac_address) {
    parts.push(`MAC: ${iface.mac_address}`);
  }
  if (ipList.length > 0) {
    parts.push(`IP: ${ipList.join(", ")}`);
  }
  if (iface.vlan_id) {
    parts.push(`${t("device.vlan_id")}: ${iface.vlan_id}`);
  }
  if (iface.port_type) {
    const typeKey = `device.port_type_${iface.port_type}`;
    const typeText = t(typeKey);
    parts.push(`${t("device.port_type")}: ${typeText === typeKey ? iface.port_type : typeText}`);
  }
  if (iface.status && iface.status !== "up") {
    parts.push(`${t("ip.status")}: ${iface.status}`);
  }
  if (iface.speed) {
    parts.push(`${t("device.speed")}: ${iface.speed}`);
  }
  return parts.join(" | ");
}

// ==================== 端口详情弹窗（新增/编辑） ====================

/** 端口详情弹窗的行数据（portId 为空 = 新增） */
interface PortFormSource {
  portId: string;
  deviceId: string;
  name: string;
  physicalType: string;
  interfaceRole: string;
  portType: string;
  status: string;
  /** 数字输入回显：空为空串 */
  nativeVlanId: number | "";
  vlanId: number | "";
  speed: string;
  macAddress: string;
  deviceManaged: boolean;
  description: string;
}

const portDetail = ref<PortFormSource | null>(null);

function openPortDetail(iface: NicPort | null): void {
  portDetail.value = {
    portId: iface?.id || "",
    deviceId: props.deviceId,
    name: iface?.name || "",
    physicalType: iface?.physical_type || "other",
    interfaceRole: iface?.interface_role || "business",
    vlanId: iface?.vlan_id || "",
    macAddress: iface?.mac_address || "",
    description: iface?.description || "",
    deviceManaged: iface?.device_managed === true,
    portType: iface?.port_type || "access",
    status: iface?.status || "up",
    speed: iface?.speed || "",
    nativeVlanId: iface?.native_vlan_id || ""
  };
}

/** 详情弹窗保存/删除成功：关闭详情并刷新分组视图 */
function onPortSaved(): void {
  portDetail.value = null;
  queryClient.invalidateQueries({ queryKey: ["device-nics", props.deviceId] });
}

// ==================== SNMP 同步 ====================

const syncing = ref(false);

/** 冲突轮次状态：确认后整体清空 */
interface ConflictRound {
  conflicts: { snmpPort: SnmpPort; existingPort: NicPort }[];
  addCount: number;
  toAdd: SnmpPort[];
  existingMap: Map<string, NicPort>;
}

const conflictRound = ref<ConflictRound | null>(null);

async function startSnmpSync(): Promise<void> {
  if (syncing.value) {
    return;
  }
  syncing.value = true;
  try {
    // 1. 拉取 SNMP 实时端口（不落库）
    const snmpResult = await apiGet<SnmpPort[]>(
      `/api/resources/devices/${props.deviceId}/snmp-ports`
    );
    if (!snmpResult.success || !Array.isArray(snmpResult.data) || snmpResult.data.length === 0) {
      showToast(t("device.snmp_no_ports"), "warning");
      return;
    }
    const newPorts = snmpResult.data;

    // 2. 现有网口（/nics cards 展平），按名称小写建索引
    const existingResult = await apiGet<{ cards?: NicCard[] }>(
      `/api/resources/devices/${props.deviceId}/nics`
    );
    const existingMap = new Map<string, NicPort>();
    if (existingResult.success && existingResult.data) {
      for (const card of existingResult.data.cards || []) {
        for (const p of card.ports || []) {
          existingMap.set(String(p.name).toLowerCase(), p);
        }
      }
    }

    // 3. 按名称（忽略大小写）分类：无冲突直接新增，有冲突弹决策弹窗
    const toAdd: SnmpPort[] = [];
    const conflicts: { snmpPort: SnmpPort; existingPort: NicPort }[] = [];
    for (const p of newPorts) {
      const key = String(p.name).toLowerCase();
      const existing = existingMap.get(key);
      if (existing) {
        conflicts.push({ snmpPort: p, existingPort: existing });
      } else {
        toAdd.push(p);
      }
    }

    if (conflicts.length === 0) {
      await applySnmpResults(toAdd, [], 0, null);
      return;
    }
    conflictRound.value = { conflicts, addCount: toAdd.length, toAdd, existingMap };
  } catch (error) {
    // 与原实现一致：非 Error 抛出值与空 message 均回退到通用失败文案
    const message = error instanceof Error ? error.message : "";
    showToast(message || t("device.port_sync_failed"), "error");
  } finally {
    syncing.value = false;
  }
}

/** 应用冲突决策：覆盖列表 + 跳过数由父级计算后进入同步任务表 */
function onConflictConfirm(decisions: Record<string, "skip" | "overwrite">): void {
  const round = conflictRound.value;
  if (!round) {
    return;
  }
  const toOverwrite = round.conflicts
    .filter((c) => decisions[String(c.snmpPort.name)] === "overwrite")
    .map((c) => c.snmpPort);
  const skippedCount = round.conflicts.length - toOverwrite.length;
  conflictRound.value = null;
  applySnmpResults(round.toAdd, toOverwrite, skippedCount, round.existingMap);
}

/** 覆盖同步载荷：仅运行属性与描述。DeviceInterfaceUpdate 的双层 Option 语义为
 * 缺失=不修改，故不补 device_managed/mac_address 等键（保持原请求键集） */
type OverwritePayload = Pick<
  DeviceInterfaceUpdate,
  "description" | "port_type" | "vlan_id" | "status" | "speed"
>;

/**
 * 执行同步任务表：先全部新增，后覆盖/退化新增；端口逐个顺序提交避免
 * 服务端限流。完成后按 新增/覆盖/跳过/失败 汇总提示并刷新分组视图。
 */
async function applySnmpResults(
  toAdd: SnmpPort[],
  toOverwrite: SnmpPort[],
  skippedCount: number,
  existingMap: Map<string, NicPort> | null
): Promise<void> {
  let added = 0;
  let overwritten = 0;
  let failed = 0;

  // 新增：网卡按端口名前缀由后端自动生成，不进设备模态框
  const buildCreatePayload = (p: SnmpPort): DeviceInterfaceCreate => ({
    name: p.name,
    // nic_id/mac_address/native_vlan_id 为原代码缺省键：显式 null 与缺省在后端 serde Option 下等价
    nic_id: null,
    mac_address: null,
    native_vlan_id: null,
    physical_type: "other",
    interface_role: "business",
    description: p.description || null,
    port_type: p.port_type || null,
    vlan_id: p.vlan_id ?? null,
    status: p.status || null,
    speed: p.speed || null,
    device_managed: false
  });

  // 覆盖：仅更新运行属性与描述，不传 device_managed（保留原托管状态）
  const buildOverwritePayload = (p: SnmpPort): OverwritePayload => ({
    description: p.description || null,
    port_type: p.port_type || null,
    vlan_id: p.vlan_id ?? null,
    status: p.status || null,
    speed: p.speed || null
  });

  // 任务表 + 递归执行器逐个提交，失败仅计数由汇总提示体现
  const tasks: Array<() => Promise<void>> = [];
  for (const p of toAdd) {
    tasks.push(async () => {
      const r = await apiPost(
        `/api/resources/devices/${props.deviceId}/interfaces`,
        buildCreatePayload(p)
      );
      if (r.success) {
        added++;
      } else {
        failed++;
      }
    });
  }
  for (const p of toOverwrite) {
    tasks.push(async () => {
      const existing = existingMap?.get(String(p.name).toLowerCase());
      if (!existing) {
        // 本轮快照中找不到原网口时退化为新增
        const r = await apiPost(
          `/api/resources/devices/${props.deviceId}/interfaces`,
          buildCreatePayload(p)
        );
        if (r.success) {
          added++;
        } else {
          failed++;
        }
        return;
      }
      const r = await apiPut(
        `/api/resources/devices/interfaces/${existing.id}`,
        buildOverwritePayload(p)
      );
      if (r.success) {
        overwritten++;
      } else {
        failed++;
      }
    });
  }

  const runTasks = async (i = 0): Promise<void> => {
    if (i >= tasks.length) {
      return;
    }
    const task = tasks[i];
    try {
      if (task) {
        await task();
      }
    } catch {
      failed++;
    }
    await runTasks(i + 1);
  };
  await runTasks();

  const parts: string[] = [];
  if (added > 0) {
    parts.push(`${t("device.sync_added")} ${added}`);
  }
  if (overwritten > 0) {
    parts.push(`${t("device.sync_overwritten")} ${overwritten}`);
  }
  if (skippedCount > 0) {
    parts.push(`${t("device.sync_skipped")} ${skippedCount}`);
  }
  if (failed > 0) {
    parts.push(`${t("device.sync_failed")} ${failed}`);
  }
  showToast(`${t("device.sync_result")}: ${parts.join(", ")}`, failed > 0 ? "warning" : "success");

  queryClient.invalidateQueries({ queryKey: ["device-nics", props.deviceId] });
}
</script>

<template>
  <ModalShell size="xl" @close="emit('close')">
    <template #title>{{ props.deviceName }} - {{ t("device.unified_ports") }}</template>
    <div class="port-groups-container">
      <p v-if="nicsQuery.isLoading.value" class="empty-message">{{ t("common.loading") }}</p>
      <template v-else>
        <div
          v-for="card in cards.filter((c) => (c.ports || []).length > 0)"
          :key="card.id"
          class="port-group is-switch-group"
        >
          <h4>{{ card.name || t("device.network_card") }} ({{ (card.ports || []).length }})</h4>
          <div class="port-grid">
            <div
              v-for="iface in sortedPorts(card)"
              :key="iface.id"
              class="port-item"
              :class="[iface.device_managed ? 'nic-interface' : '', `status-${iface.status || 'up'}`]"
              @click="openPortDetail(iface)"
            >
              {{ extractPortLastNumber(iface.name) || (iface.name || "?").slice(0, 3) }}
              <div class="port-tooltip">{{ portTooltip(iface) }}</div>
            </div>
          </div>
        </div>
        <p v-if="cards.every((c) => (c.ports || []).length === 0)" class="empty-message">
          {{ t("device.no_device_ports") }}
        </p>
      </template>
    </div>
    <template #footer>
      <button type="button" class="btn btn-primary" @click="openPortDetail(null)">
        {{ t("device.add_device_port") }}
      </button>
      <button type="button" class="btn btn-secondary" :disabled="syncing" @click="startSnmpSync">
        {{ syncing ? t("device.fetching_snmp") : t("device.sync_snmp_ports") }}
      </button>
    </template>

    <!-- 端口详情（新增/编辑） -->
    <PortDetailModal v-if="portDetail" :port="portDetail" @close="portDetail = null" @saved="onPortSaved" />

    <!-- SNMP 端口名称冲突决策 -->
    <PortConflictModal
      v-if="conflictRound"
      :conflicts="conflictRound.conflicts"
      :add-count="conflictRound.addCount"
      @close="conflictRound = null"
      @confirm="onConflictConfirm"
    />
  </ModalShell>
</template>
