<script setup lang="ts">
/**
 * 拉取 MAC 弹窗：
 * 左列 SNMP 设备复选列表（勾选一个或多个作为本次拉取设备），右列目标子网
 * 下拉（空值 = 全部子网）。设备/子网/默认拉取配置三查询并行，就绪后按
 * 服务端保存的上次拉取配置预选（设备与现存可选项求交，已删除的剔除）。
 * 提交按勾选顺序依次同步（后端语义），成功后刷新 IP 列表。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery } from "@tanstack/vue-query";
import { apiGet, apiPost } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";
import type { Paged } from "../../types/api";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";
import type { MacPullDefaults } from "../../types/api/generated/MacPullDefaults";
import type { Network } from "../../types/api/generated/Network";
import type { PullIpDetailsRequest } from "../../types/api/generated/PullIpDetailsRequest";

/** 设备列表行：后端在 DeviceWithDetails 基础上注入 snmp_configured 标记 */
type PullDeviceItem = DeviceWithDetails & {
  snmp_configured: boolean;
};

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();

// 设备列表：page_size=1000 全量拉取
const devicesQuery = useQuery({
  queryKey: ["ip-pull-devices"],
  queryFn: async (): Promise<Paged<PullDeviceItem> | null> => {
    const result = await apiGet<Paged<PullDeviceItem>>("/api/resources/devices?page_size=1000");
    if (!result.success) {
      throw new Error(result.message || t("ip.load_failed_short"));
    }
    return result.data;
  }
});

// 网段列表：page_size=1000 全量拉取
const networksQuery = useQuery({
  queryKey: ["ip-pull-networks"],
  queryFn: async (): Promise<Paged<Network> | null> => {
    const result = await apiGet<Paged<Network>>("/api/resources/networks?page_size=1000");
    if (!result.success) {
      throw new Error(result.message || t("ip.load_failed_short"));
    }
    return result.data;
  }
});

// 默认拉取配置：服务端保存的上次拉取设备集合与子网；读取失败不阻塞弹窗，
// 仅不预选
const defaultsQuery = useQuery({
  queryKey: ["ip-pull-defaults"],
  queryFn: async (): Promise<MacPullDefaults> => {
    const result = await apiGet<MacPullDefaults | null>("/api/resources/ip/pull-defaults");
    return result.success && result.data
      ? {
          device_ids: result.data.device_ids || [],
          subnet_id: result.data.subnet_id || null
        }
      : { device_ids: [], subnet_id: null };
  }
});

// SNMP 设备：snmp_community 列表响应已脱敏为 null，判定用 snmp_configured 标记
const snmpDevices = computed(() =>
  (devicesQuery.data.value?.items ?? []).filter((dev) => dev.snmp_configured)
);

// 设备列表占位文案（按优先级判定）：网络异常 → 加载失败；空列表 →
// 暂无设备；无 SNMP 设备 → 暂无配置 SNMP 的设备
const devicePlaceholder = computed(() => {
  if (devicesQuery.isError.value) {
    return t("ip.load_failed_short");
  }
  const items = devicesQuery.data.value?.items ?? [];
  if (items.length === 0) {
    return t("ip.no_device_data");
  }
  if (snmpDevices.value.length === 0) {
    return t("ip.no_snmp_device");
  }
  return "";
});

// 子网选择值：空串 = 全部子网；默认子网已被删除时 select 赋值自然回落空值
const subnetId = ref("");

// 勾选集合（设备数据与默认配置就绪后预选一次）
const checkedIds = ref(new Set<string>());
let defaultsApplied = false;

watch(
  [snmpDevices, defaultsQuery.data],
  ([devices, defaults]) => {
    if (defaultsApplied || !defaults || devices.length === 0) {
      return;
    }
    const validIds = new Set(devices.map((dev) => dev.id));
    checkedIds.value = new Set(defaults.device_ids.filter((id) => validIds.has(id)));
    if (defaults.subnet_id) {
      subnetId.value = defaults.subnet_id;
    }
    defaultsApplied = true;
  },
  { immediate: true }
);

function toggleDevice(id: string, checked: boolean): void {
  const next = new Set(checkedIds.value);
  if (checked) {
    next.add(id);
  } else {
    next.delete(id);
  }
  checkedIds.value = next;
}

/** input change 事件目标断言为复选框元素（模板 @change 内联调用） */
function isChecked(event: Event): boolean {
  return event.target instanceof HTMLInputElement && event.target.checked;
}

// 子网下拉数据：正常时占位选项（全部子网）+ 全量网段；
// 异常/空数据时仅保留占位选项（异常文案或全部子网）
const networkOptions = computed(() => {
  if (networksQuery.isError.value) {
    return null;
  }
  const items = networksQuery.data.value?.items ?? [];
  if (items.length === 0) {
    return null;
  }
  return items.map((network) => ({
    id: network.id,
    label: `${network.name} (${network.ipv4_cidr || network.ipv6_cidr || "-"})`
  }));
});

const pullMutation = useMutation({
  mutationFn: (payload: PullIpDetailsRequest) => apiPost("/api/resources/ip/pull", payload),
  onSuccess: (result) => {
    if (result.success) {
      showToast(result.message || t("ip.pull_mac_success"), "success");
      emit("saved");
      emit("close");
    } else {
      showToast(`${t("ip.pull_mac_failed")}: ${result.message}`, "error");
    }
  },
  onError: (error: Error) => {
    showToast(error.message || t("ip.pull_mac_failed"), "error");
  }
});

function submitPull(): void {
  if (checkedIds.value.size === 0) {
    showToast(t("ip.select_device_first"), "warning");
    return;
  }
  // 子网可缺省：空值表示全部子网，后端不做 ips.subnet_id 过滤；
  // 显式 null 与原条件缺省键在后端 serde Option 下等价
  const payload: PullIpDetailsRequest = {
    device_ids: [...checkedIds.value],
    subnet_id: subnetId.value || null
  };
  pullMutation.mutate(payload);
}

// 设备显示名：名称优先，其次主机名，最后回退 id
const deviceLabel = (dev: PullDeviceItem): string => dev.name || dev.hostname || dev.id;
</script>

<template>
  <ModalShell :title="t('ip.pull_mac')" @close="emit('close')">
    <form id="pull-mac-form" @submit.prevent="submitPull">
      <!-- 两列布局：左侧设备复选列表，右侧子网下拉（窄屏 form-row auto-fit 自动回落单列） -->
      <div class="form-row">
        <div class="form-group form-group-block">
          <span class="group-label">{{ t("ip.select_devices") }}</span>
          <div v-if="devicePlaceholder" class="users-checkbox-list">
            <div class="user-checkbox-item">{{ devicePlaceholder }}</div>
          </div>
          <div v-else class="users-checkbox-list">
            <div v-for="dev in snmpDevices" :key="dev.id" class="user-checkbox-item">
              <input
                type="checkbox"
                :id="`pull-mac-device-${dev.id}`"
                :value="dev.id"
                :checked="checkedIds.has(dev.id)"
                @change="toggleDevice(dev.id, isChecked($event))"
              >
              <label :for="`pull-mac-device-${dev.id}`" class="user-info">
                <span class="user-name">{{ deviceLabel(dev) }}</span>
              </label>
            </div>
          </div>
        </div>
        <div class="form-group form-group-block">
          <label for="pull-mac-network-select">{{ t("ip.select_network") }}</label>
          <select id="pull-mac-network-select" v-model="subnetId" name="subnet_id" :aria-label="t('ip.select_network')">
            <option value="">
              {{ networksQuery.isError.value ? t("ip.load_failed_short") : t("ip.all_networks") }}
            </option>
            <template v-if="networkOptions">
              <option v-for="network in networkOptions" :key="network.id" :value="network.id">
                {{ network.label }}
              </option>
            </template>
          </select>
        </div>
      </div>
    </form>
    <template #footer>
      <button type="submit" class="btn btn-primary" form="pull-mac-form" :disabled="pullMutation.isPending.value">
        <span v-if="pullMutation.isPending.value" class="loading" aria-hidden="true"></span>
        {{ pullMutation.isPending.value ? t("ip.pulling") : t("common.confirm") }}
      </button>
    </template>
  </ModalShell>
</template>

<style scoped>
/* 复选列表样式（本弹窗专用） */
.users-checkbox-list {
  max-height: 200px;
  overflow-y: auto;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  padding: 0.5rem;
  background: var(--color-bg);
}

.users-checkbox-list .user-checkbox-item {
  display: flex;
  align-items: center;
  padding: 0.5rem;
  border-radius: var(--radius-sm);
  transition: background-color 0.2s;
}

.users-checkbox-list .user-checkbox-item:hover {
  background-color: var(--color-bg-white);
}

.users-checkbox-list input[type="checkbox"] {
  margin-right: 0.75rem;
  width: 18px;
  height: 18px;
}

.users-checkbox-list .user-info {
  display: flex;
  flex-direction: column;
}

.users-checkbox-list .user-name {
  font-weight: 500;
  color: var(--color-text);
}

.users-checkbox-list .user-email {
  font-size: 0.85rem;
  color: var(--color-text-muted);
}
</style>
