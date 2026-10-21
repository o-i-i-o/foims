<script setup lang="ts">
/**
 * 端口详情弹窗（移植自旧版 modals/device/device-port-detail-modal.html +
 * unifiedDevicePorts.js openPortDetailModal/collectPortFormData/submitPortForm/
 * deletePort）：统一端口管理的新增/编辑表单。
 * - 配置项与设备模态框网口容器一致：名称/物理形态/接口角色/端口类型/状态/
 *   Native VLAN/VLAN/速率/MAC/设备管理/描述；
 * - 「设备管理」勾选后该网口进入设备编辑模态框管理；端口模态框新建的端口
 *   默认不勾选；
 * - 校验：端口名必填、VLAN/Native VLAN 1-4094；
 * - 保存：有 id 走 PUT /devices/interfaces/{id}，否则 POST /devices/{deviceId}/
 *   interfaces；删除需确认，成功后 emit("saved") 由父组件关闭并刷新。
 */
import { reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation } from "@tanstack/vue-query";
import { apiDelete, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { INTERFACE_ROLES, PHYSICAL_TYPES } from "../../config/deviceOptions";
import ConfirmModal from "../ConfirmModal.vue";
import ModalShell from "../ModalShell.vue";
import type { DeviceInterfaceUpdate } from "../../types/api/generated/DeviceInterfaceUpdate";

/** 打开端口的行数据（由统一端口管理弹窗构造；portId 为空 = 新增） */
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

const props = defineProps<{
  /** 打开端口的完整数据；portId 为空 = 新增（默认 access/up、不托管） */
  port: PortFormSource;
}>();

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();

const PORT_TYPE_OPTIONS: { value: string; labelKey: string }[] = [
  { value: "access", labelKey: "device.port_type_access" },
  { value: "trunk", labelKey: "device.port_type_trunk" },
  { value: "hybrid", labelKey: "device.port_type_hybrid" },
  { value: "uplink", labelKey: "device.port_type_uplink" },
  { value: "stack", labelKey: "device.port_type_stack" },
  { value: "console", labelKey: "device.port_type_console" }
];

const PORT_STATUS_OPTIONS: { value: string; labelKey: string }[] = [
  { value: "up", labelKey: "device.port_status_up" },
  { value: "down", labelKey: "device.port_status_down" },
  { value: "admin-down", labelKey: "device.port_status_admin_down" }
];

const isNewPort = !props.port.portId;

// ==================== 表单状态 ====================

const form = reactive({
  name: props.port.name || "",
  physicalType: props.port.physicalType || "other",
  interfaceRole: props.port.interfaceRole || "business",
  portType: props.port.portType || "access",
  status: props.port.status || "up",
  nativeVlanId: props.port.nativeVlanId ?? "",
  vlanId: props.port.vlanId ?? "",
  speed: props.port.speed || "",
  macAddress: props.port.macAddress || "",
  deviceManaged: props.port.deviceManaged === true,
  description: props.port.description || ""
});

// VLAN 输入归一：有值时 parseInt，非法范围提交前校验拦截
function parseVlanInput(raw: number | "" | null | undefined): number | null {
  if (raw === "" || raw === null || raw === undefined) {
    return null;
  }
  return Number.parseInt(String(raw), 10);
}

function isInvalidVlan(value: number | null): boolean {
  return value !== null && (Number.isNaN(value) || value < 1 || value > 4094);
}

// ==================== 保存 ====================

const saveMutation = useMutation({
  mutationFn: async () => {
    const name = form.name.trim();
    if (!name) {
      showToast(t("device.port_name_required"), "warning");
      return null;
    }
    const vlanId = parseVlanInput(form.vlanId);
    if (isInvalidVlan(vlanId)) {
      showToast(t("device.vlan_invalid"), "warning");
      return null;
    }
    const nativeVlanId = parseVlanInput(form.nativeVlanId);
    if (isInvalidVlan(nativeVlanId)) {
      showToast(t("device.native_vlan_invalid"), "warning");
      return null;
    }
    // 创建/更新共用载荷：按 DeviceInterfaceUpdate 键集（与原发送键集一致，
    // 不补 nic_id——后端缺省时按端口名前缀自动归属网卡）
    const payload: DeviceInterfaceUpdate = {
      name,
      physical_type: form.physicalType || "other",
      interface_role: form.interfaceRole || "business",
      vlan_id: vlanId,
      mac_address: form.macAddress.trim() || null,
      description: form.description.trim() || null,
      device_managed: form.deviceManaged,
      port_type: form.portType || "access",
      status: form.status || "up",
      speed: form.speed.trim() || null,
      native_vlan_id: nativeVlanId
    };
    const result = props.port.portId
      ? await apiPut(`/api/resources/devices/interfaces/${props.port.portId}`, payload)
      : await apiPost(`/api/resources/devices/${props.port.deviceId}/interfaces`, payload);
    if (!result.success) {
      throw new Error(`${t("device.port_save_failed")}: ${result.message || ""}`);
    }
    return result;
  },
  onSuccess: (result) => {
    if (!result) {
      return; // 校验未通过，已提示
    }
    showToast(props.port.portId ? t("device.port_update_success") : t("device.port_add_success"), "success");
    emit("saved");
  },
  onError: (error) => {
    showToast(error.message || t("device.port_save_failed"), "error");
  }
});

// ==================== 删除（仅编辑态） ====================

const confirmingDelete = ref(false);

const deleteMutation = useMutation({
  mutationFn: async () => {
    const result = await apiDelete(`/api/resources/devices/interfaces/${props.port.portId}`);
    if (!result.success) {
      throw new Error(`${t("device.port_delete_failed")}: ${result.message || ""}`);
    }
    return result;
  },
  onSuccess: () => {
    showToast(t("device.port_delete_success"), "success");
    emit("saved");
  },
  onError: (error) => {
    showToast(error.message || t("device.port_delete_failed"), "error");
  }
});
</script>

<template>
  <ModalShell
    :title="isNewPort ? t('device.add_port') : `${t('device.port_detail')} - ${props.port.name}`"
    @close="emit('close')"
  >
    <form @submit.prevent="saveMutation.mutate()">
      <div class="form-row">
        <div class="form-group">
          <label for="device-port-name">
            <span>{{ t("device.network_port_name") }}</span><span class="required">*</span>
          </label>
          <input
            id="device-port-name"
            v-model="form.name"
            type="text"
            required
            autocomplete="off"
            placeholder="e.g., eth0, xg1/0/0/1"
          />
        </div>
        <div class="form-group">
          <label for="device-port-ptype">{{ t("device.physical_type") }}</label>
          <select id="device-port-ptype" v-model="form.physicalType">
            <option v-for="opt in PHYSICAL_TYPES" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-port-role">{{ t("device.interface_role") }}</label>
          <select id="device-port-role" v-model="form.interfaceRole">
            <option v-for="opt in INTERFACE_ROLES" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="device-port-porttype">{{ t("device.port_type") }}</label>
          <select id="device-port-porttype" v-model="form.portType">
            <option v-for="opt in PORT_TYPE_OPTIONS" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-port-status">{{ t("ip.status") }}</label>
          <select id="device-port-status" v-model="form.status">
            <option v-for="opt in PORT_STATUS_OPTIONS" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="device-port-nativevlan" :title="t('device.native_vlan_id_hint')">
            {{ t("device.native_vlan_id") }}
          </label>
          <input id="device-port-nativevlan" v-model="form.nativeVlanId" type="number" min="1" max="4094" autocomplete="off" />
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-port-vlan">{{ t("device.vlan_id") }}</label>
          <input id="device-port-vlan" v-model="form.vlanId" type="number" min="1" max="4094" autocomplete="off" />
        </div>
        <div class="form-group">
          <label for="device-port-speed">{{ t("device.speed") }}</label>
          <input id="device-port-speed" v-model="form.speed" type="text" autocomplete="off" placeholder="e.g., 1000Mbps" />
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-port-mac">{{ t("ip.mac_address") }}</label>
          <input id="device-port-mac" v-model="form.macAddress" type="text" autocomplete="off" placeholder="00:11:22:33:44:55" />
        </div>
        <div class="form-group form-group-inline">
          <label for="device-port-managed" :title="t('device.device_managed_hint')">{{ t("device.device_managed") }}</label>
          <input id="device-port-managed" v-model="form.deviceManaged" type="checkbox" class="form-checkbox" />
        </div>
      </div>
      <div class="form-group form-group-block">
        <textarea
          v-model="form.description"
          :aria-label="t('common.description')"
          :placeholder="t('common.description_placeholder')"
        ></textarea>
      </div>
    </form>
    <template #footer>
      <button
        v-if="!isNewPort"
        type="button"
        class="btn btn-danger"
        :disabled="deleteMutation.isPending.value"
        @click="confirmingDelete = true"
      >
        {{ deleteMutation.isPending.value ? t("common.deleting") : t("common.delete") }}
      </button>
      <button
        type="button"
        class="btn btn-primary"
        :disabled="saveMutation.isPending.value"
        @click="saveMutation.mutate()"
      >
        {{ saveMutation.isPending.value ? t("common.saving") : t("common.save") }}
      </button>
    </template>

    <!-- 删除确认 -->
    <ConfirmModal
      v-if="confirmingDelete"
      :message="t('device.confirm_delete_port')"
      danger
      @confirm="deleteMutation.mutate()"
      @cancel="confirmingDelete = false"
    />
  </ModalShell>
</template>
