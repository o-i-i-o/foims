<script setup lang="ts">
/**
 * 线路新增/编辑弹窗：
 * - A/B 两端对称级联（createSide 工厂复用）：端点类型 → 范围 → 端点；
 *   设备接口流程为 房间 → 机柜（可选「不限机柜」）→ 设备 → 端点（接口/端口）；
 * - 范围选择器：信息点/设备接口按房间、配线架按机柜（选项来自
 *   /options/rooms|cabinets|devices）；
 * - 端点下拉：信息点 /options/net-outlets、配线架 /patch-panels、设备接口
 *   /devices/{id}/interfaces（过滤 virtual 形态，选项值前缀编码
 *   device_interface:<id>，提交时还原真实类型）；
 * - 级联加载按端独立序号守卫，快速切换时丢弃过期响应；
 * - 编辑回填：详情已含端点所属房间/机柜/设备，按类型驱动级联逐级回填。
 */
import { reactive, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery } from "@tanstack/vue-query";
import type { CableLinkCreate } from "../../types/api/generated/CableLinkCreate";
import type { CableLinkWithDetails } from "../../types/api/generated/CableLinkWithDetails";
import type { DeviceInterface } from "../../types/api/generated/DeviceInterface";
import type { PatchPanel } from "../../types/api/generated/PatchPanel";
import type { ResourceOption } from "../../types/api/generated/ResourceOption";
import type { ApiResult, Paged } from "../../types/api";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";

const props = withDefaults(
  defineProps<{
    /** "" = 新增；非空 = 编辑（GET /cable-links/{id} 回填） */
    linkId?: string;
  }>(),
  { linkId: "" }
);

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();

/** 带翻译键的下拉选项 */
interface LabeledOption {
  value: string;
  labelKey: string;
}

const ENDPOINT_TYPE_OPTIONS: LabeledOption[] = [
  { value: "net_outlet", labelKey: "cable_link.endpoint_net_outlet" },
  { value: "device_interface", labelKey: "cable_link.endpoint_device_interface" },
  { value: "patch_panel", labelKey: "cable_link.endpoint_patch_panel" }
];

const LINK_TYPE_OPTIONS: LabeledOption[] = [
  { value: "ethernet", labelKey: "cable_link.link_type_ethernet" },
  { value: "fiber", labelKey: "cable_link.link_type_fiber" },
  { value: "console", labelKey: "cable_link.link_type_console" }
];

// 设备接口下拉值的前缀，提交时据此还原真实 endpoint_type
const DEVICE_INTERFACE_PREFIX = "device_interface:";

/** 端点下拉选项（value 支持设备接口前缀编码） */
interface EndpointOption {
  value: string;
  label: string;
}

/** 端点级联单侧状态（A/B 两端对称，createSide 工厂复用） */
interface CableSideState {
  type: string;
  scope: string;
  cabinetId: string;
  deviceId: string;
  endpointId: string;
  scopeOptions: ResourceOption[];
  cabinetOptions: ResourceOption[];
  deviceOptions: ResourceOption[];
  endpointOptions: EndpointOption[];
  // 级联加载序号：快速切换时旧响应晚到不得覆盖新选项
  seq: { scope: number; cabinet: number; device: number; endpoint: number };
}

// ==================== 端点级联（A/B 两端对称） ====================

function createSide(): CableSideState {
  return reactive<CableSideState>({
    type: "net_outlet",
    scope: "",
    cabinetId: "",
    deviceId: "",
    endpointId: "",
    // 选项列表
    scopeOptions: [],
    cabinetOptions: [],
    deviceOptions: [],
    endpointOptions: [],
    seq: { scope: 0, cabinet: 0, device: 0, endpoint: 0 }
  });
}

const sideA = createSide();
const sideB = createSide();

const form = reactive({
  linkType: "ethernet",
  cableLabel: "",
  length: "",
  tested: "0"
});

// 设备接口流程第二行（机柜+设备）仅该类型显示
function isDeviceFlow(side: CableSideState): boolean {
  return side.type === "device_interface";
}

/** 范围标签：设备接口/信息点按房间，配线架按机柜 */
function scopeLabelKey(side: CableSideState): string {
  return side.type === "patch_panel" ? "cable_link.scope_cabinet" : "cable_link.scope_room";
}

/** 范围占位：配线架提示选机柜，其余提示选房间 */
function scopePlaceholderKey(side: CableSideState): string {
  return side.type === "patch_panel" ? "cable_link.select_cabinet" : "cable_link.select_room";
}

/** 从 ApiResult 提取选项数组（兼容裸数组与分页信封两种载荷） */
function extractItems<T>(result: ApiResult<T[] | Paged<T>>): T[] {
  const data = result.success ? result.data : null;
  if (Array.isArray(data)) {
    return data;
  }
  const items = data?.items;
  return Array.isArray(items) ? items : [];
}

// 类型切换：清空端点与范围，重置级联并按类型加载范围选项
async function onTypeChange(side: CableSideState): Promise<void> {
  side.endpointId = "";
  side.scope = "";
  side.cabinetId = "";
  side.deviceId = "";
  side.scopeOptions = [];
  side.cabinetOptions = [];
  side.deviceOptions = [];
  side.endpointOptions = [];
  if (!side.type) {
    return;
  }
  const seq = ++side.seq.scope;
  const url =
    side.type === "patch_panel" ? "/api/resources/options/cabinets" : "/api/resources/options/rooms";
  const result = await apiGet<ResourceOption[] | Paged<ResourceOption>>(url);
  if (seq !== side.seq.scope) {
    return;
  }
  side.scopeOptions = extractItems(result);
}

// 范围切换：信息点/配线架直接加载端点；设备接口加载机柜（不限）与设备
async function onScopeChange(side: CableSideState): Promise<void> {
  side.endpointId = "";
  side.endpointOptions = [];
  if (side.type === "device_interface") {
    await Promise.all([loadCabinetOptions(side), loadDeviceOptions(side, { roomId: side.scope })]);
    return;
  }
  await loadEndpointOptions(side, side.scope);
}

// 机柜切换：按机柜过滤设备，「不限机柜」时回到房间范围
async function onCabinetChange(side: CableSideState): Promise<void> {
  side.endpointId = "";
  side.endpointOptions = [];
  await loadDeviceOptions(side, side.cabinetId ? { cabinetId: side.cabinetId } : { roomId: side.scope });
}

// 设备切换：加载该设备的全部网口（统一端口模型）
async function onDeviceChange(side: CableSideState): Promise<void> {
  side.endpointId = "";
  side.endpointOptions = [];
  if (side.deviceId) {
    await loadEndpointOptions(side, side.deviceId);
  }
}

async function loadCabinetOptions(side: CableSideState): Promise<void> {
  const seq = ++side.seq.cabinet;
  if (!side.scope) {
    side.cabinetOptions = [];
    return;
  }
  const result = await apiGet<ResourceOption[] | Paged<ResourceOption>>(`/api/resources/options/cabinets?room_id=${side.scope}`);
  if (seq !== side.seq.cabinet) {
    return;
  }
  side.cabinetOptions = extractItems(result);
}

async function loadDeviceOptions(
  side: CableSideState,
  { roomId = null, cabinetId = null }: { roomId?: string | null; cabinetId?: string | null } = {}
): Promise<void> {
  const seq = ++side.seq.device;
  if (!roomId && !cabinetId) {
    side.deviceOptions = [];
    return;
  }
  const query = cabinetId ? `cabinet_id=${cabinetId}` : `room_id=${roomId}`;
  const result = await apiGet<ResourceOption[] | Paged<ResourceOption>>(`/api/resources/options/devices?${query}`);
  if (seq !== side.seq.device) {
    return;
  }
  side.deviceOptions = extractItems(result);
}

/**
 * 端点选项：scopeValue 语义随类型变化（信息点=房间、配线架=机柜、
 * 设备接口=设备 id）；设备接口值用前缀编码，提交时 decodeEndpoint 还原
 */
async function loadEndpointOptions(side: CableSideState, scopeValue: string): Promise<void> {
  const seq = ++side.seq.endpoint;
  if (!scopeValue) {
    side.endpointOptions = [];
    return;
  }
  try {
    if (side.type === "net_outlet") {
      const result = await apiGet<ResourceOption[] | Paged<ResourceOption>>(`/api/resources/options/net-outlets?room_id=${scopeValue}`);
      if (seq !== side.seq.endpoint) {
        return;
      }
      side.endpointOptions = extractItems(result).map((o) => ({ value: o.id, label: o.name }));
    } else if (side.type === "device_interface") {
      const result = await apiGet<DeviceInterface[] | Paged<DeviceInterface>>(`/api/resources/devices/${scopeValue}/interfaces?page_size=1000`);
      if (seq !== side.seq.endpoint) {
        return;
      }
      side.endpointOptions = extractItems(result)
        .filter((di) => !di.physical_type || di.physical_type !== "virtual")
        .map((di) => ({
          value: `${DEVICE_INTERFACE_PREFIX}${di.id}`,
          label: di.name || di.id
        }));
    } else if (side.type === "patch_panel") {
      const result = await apiGet<PatchPanel[] | Paged<PatchPanel>>(`/api/resources/patch-panels?cabinet_id=${scopeValue}&page_size=1000`);
      if (seq !== side.seq.endpoint) {
        return;
      }
      side.endpointOptions = extractItems(result).map((pp) => ({ value: pp.id, label: pp.name }));
    }
  } catch {
    // 端点选项加载失败仅置空列表，用户重选范围即可重试
    side.endpointOptions = [];
  }
}

// ==================== 编辑回填 ====================

const detailQuery = useQuery({
  queryKey: () => ["cable-link", props.linkId],
  queryFn: async (): Promise<CableLinkWithDetails | null> => {
    const result = await apiGet<CableLinkWithDetails>(`/api/resources/cable-links/${props.linkId}`);
    if (!result.success) {
      throw new Error(result.message || t("cable_link.load_failed"));
    }
    return result.data;
  },
  enabled: () => Boolean(props.linkId)
});

/** 端点值解码：device_interface:<id> 前缀还原真实类型与 id */
function decodeEndpoint(type: string, rawValue: string): { type: string; id: string } {
  if (!rawValue) {
    return { type: "", id: "" };
  }
  if (type === "device_interface") {
    const idx = rawValue.indexOf(":");
    if (idx > 0) {
      return { type: rawValue.slice(0, idx), id: rawValue.slice(idx + 1) };
    }
  }
  return { type, id: rawValue };
}

/** 级联回填用的端点上下文（详情已含房间/机柜/设备归属） */
interface EndpointContext {
  type: string;
  id: string;
  roomId: string | null;
  cabinetId: string | null;
  deviceId: string | null;
}

/**
 * 按端点类型驱动级联回填（类型→范围→机柜（可选）→设备→端点），
 * 与新建表单完全复用，回填后所有选择器保持可编辑
 */
async function populateEndpointCascade(side: CableSideState, ep: EndpointContext): Promise<void> {
  side.type = ep.type;
  await onTypeChange(side);

  if (ep.type === "device_interface") {
    side.scope = ep.roomId || "";
    await onScopeChange(side);
    if (ep.cabinetId) {
      side.cabinetId = ep.cabinetId;
      await onCabinetChange(side);
    }
    side.deviceId = ep.deviceId || "";
    await onDeviceChange(side);
    // 端点值为前缀编码（device_interface:<id>）
    side.endpointId = `${DEVICE_INTERFACE_PREFIX}${ep.id}`;
    return;
  }
  side.scope = (ep.type === "net_outlet" ? ep.roomId : ep.cabinetId) || "";
  await onScopeChange(side);
  side.endpointId = ep.id;
}

async function fillForEdit(link: CableLinkWithDetails): Promise<void> {
  form.linkType = link.link_type || "ethernet";
  form.cableLabel = link.cable_label || "";
  form.length = link.length_m != null ? String(link.length_m) : "";
  form.tested = link.tested ? "1" : "0";
  // 后端已随详情返回端点所属房间/机柜/设备（a_room_id 等）
  await populateEndpointCascade(sideA, {
    type: link.a_endpoint_type,
    id: link.a_endpoint_id,
    roomId: link.a_room_id,
    cabinetId: link.a_cabinet_id,
    deviceId: link.a_device_id
  });
  await populateEndpointCascade(sideB, {
    type: link.b_endpoint_type,
    id: link.b_endpoint_id,
    roomId: link.b_room_id,
    cabinetId: link.b_cabinet_id,
    deviceId: link.b_device_id
  });
}

if (props.linkId) {
  // 回填在查询数据到达后执行（加载态由查询 isPending 展示）
  watch(
    () => detailQuery.data.value,
    (link) => {
      if (link) {
        fillForEdit(link);
      }
    },
    { immediate: true }
  );
} else {
  // 新建默认信息点：初始化两端范围选项（房间）
  onTypeChange(sideA);
  onTypeChange(sideB);
}

// ==================== 提交 ====================

const saveMutation = useMutation({
  mutationFn: async (): Promise<ApiResult<CableLinkWithDetails> | null> => {
    const a = decodeEndpoint(sideA.type, sideA.endpointId);
    const b = decodeEndpoint(sideB.type, sideB.endpointId);
    if (!a.type || !a.id || !b.type || !b.id) {
      showToast(t("cable_link.endpoint_required"), "warning");
      return null;
    }
    if (a.type === b.type && a.id === b.id) {
      showToast(t("cable_link.no_self_link"), "warning");
      return null;
    }
    const payload: CableLinkCreate = {
      a_endpoint_type: a.type,
      a_endpoint_id: a.id,
      b_endpoint_type: b.type,
      b_endpoint_id: b.id,
      link_type: form.linkType || "ethernet",
      cable_label: form.cableLabel || null,
      length_m: form.length ? Number.parseFloat(form.length) : null,
      tested: form.tested === "1" || form.tested === "true"
    };
    const result = props.linkId
      ? await apiPut<CableLinkWithDetails, CableLinkCreate>(`/api/resources/cable-links/${props.linkId}`, payload)
      : await apiPost<CableLinkWithDetails, CableLinkCreate>("/api/resources/cable-links", payload);
    if (!result.success) {
      throw new Error(result.message || t("cable_link.save_failed"));
    }
    return result;
  },
  onSuccess: (result) => {
    if (!result) {
      return; // 校验未通过，已提示
    }
    showToast(t("cable_link.save_success"), "success");
    emit("saved");
  },
  onError: (error) => {
    showToast(error.message || t("cable_link.save_failed"), "error");
  }
});
</script>

<template>
  <ModalShell :title="t(props.linkId ? 'cable_link.edit' : 'cable_link.add')" @close="emit('close')">
    <div v-if="detailQuery.isLoading.value" class="modal-loading">
      <div class="spinner"></div>
      <p class="loading-text">{{ t("common.loading") }}</p>
    </div>
    <form v-else @submit.prevent="saveMutation.mutate()">
      <!-- A 端 -->
      <div class="form-row">
        <div class="form-group">
          <label for="cable-link-a-type">{{ t("cable_link.a_endpoint") }}<span class="required">*</span></label>
          <select id="cable-link-a-type" v-model="sideA.type" @change="onTypeChange(sideA)">
            <option v-for="opt in ENDPOINT_TYPE_OPTIONS" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="cable-link-a-scope">{{ t(scopeLabelKey(sideA)) }}</label>
          <select id="cable-link-a-scope" v-model="sideA.scope" @change="onScopeChange(sideA)">
            <option value="">{{ t(scopePlaceholderKey(sideA)) }}</option>
            <option v-for="item in sideA.scopeOptions" :key="item.id" :value="item.id">{{ item.name }}</option>
          </select>
        </div>
      </div>
      <div v-if="isDeviceFlow(sideA)" class="form-row">
        <div class="form-group">
          <label for="cable-link-a-cabinet">{{ t("cable_link.cabinet_filter") }}</label>
          <select id="cable-link-a-cabinet" v-model="sideA.cabinetId" @change="onCabinetChange(sideA)">
            <option value="">{{ t("cable_link.cabinet_any") }}</option>
            <option v-for="item in sideA.cabinetOptions" :key="item.id" :value="item.id">{{ item.name }}</option>
          </select>
        </div>
        <div class="form-group">
          <label for="cable-link-a-device">{{ t("cable_link.scope_device") }}</label>
          <select id="cable-link-a-device" v-model="sideA.deviceId" @change="onDeviceChange(sideA)">
            <option value="">{{ t("cable_link.select_device") }}</option>
            <option v-for="item in sideA.deviceOptions" :key="item.id" :value="item.id">
              {{ item.name || item.id }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="cable-link-a-id">{{ t("cable_link.a_endpoint_id") }}<span class="required">*</span></label>
          <select id="cable-link-a-id" v-model="sideA.endpointId">
            <option value="">{{ t("cable_link.select_endpoint") }}</option>
            <optgroup
              v-if="sideA.type === 'device_interface' && sideA.endpointOptions.length > 0"
              :label="t('cable_link.endpoint_device_interface')"
            >
              <option v-for="opt in sideA.endpointOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
            </optgroup>
            <template v-else>
              <option v-for="opt in sideA.endpointOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
            </template>
          </select>
        </div>
      </div>
      <!-- B 端 -->
      <div class="form-row">
        <div class="form-group">
          <label for="cable-link-b-type">{{ t("cable_link.b_endpoint") }}<span class="required">*</span></label>
          <select id="cable-link-b-type" v-model="sideB.type" @change="onTypeChange(sideB)">
            <option v-for="opt in ENDPOINT_TYPE_OPTIONS" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="cable-link-b-scope">{{ t(scopeLabelKey(sideB)) }}</label>
          <select id="cable-link-b-scope" v-model="sideB.scope" @change="onScopeChange(sideB)">
            <option value="">{{ t(scopePlaceholderKey(sideB)) }}</option>
            <option v-for="item in sideB.scopeOptions" :key="item.id" :value="item.id">{{ item.name }}</option>
          </select>
        </div>
      </div>
      <div v-if="isDeviceFlow(sideB)" class="form-row">
        <div class="form-group">
          <label for="cable-link-b-cabinet">{{ t("cable_link.cabinet_filter") }}</label>
          <select id="cable-link-b-cabinet" v-model="sideB.cabinetId" @change="onCabinetChange(sideB)">
            <option value="">{{ t("cable_link.cabinet_any") }}</option>
            <option v-for="item in sideB.cabinetOptions" :key="item.id" :value="item.id">{{ item.name }}</option>
          </select>
        </div>
        <div class="form-group">
          <label for="cable-link-b-device">{{ t("cable_link.scope_device") }}</label>
          <select id="cable-link-b-device" v-model="sideB.deviceId" @change="onDeviceChange(sideB)">
            <option value="">{{ t("cable_link.select_device") }}</option>
            <option v-for="item in sideB.deviceOptions" :key="item.id" :value="item.id">
              {{ item.name || item.id }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="cable-link-b-id">{{ t("cable_link.b_endpoint_id") }}<span class="required">*</span></label>
          <select id="cable-link-b-id" v-model="sideB.endpointId">
            <option value="">{{ t("cable_link.select_endpoint") }}</option>
            <optgroup
              v-if="sideB.type === 'device_interface' && sideB.endpointOptions.length > 0"
              :label="t('cable_link.endpoint_device_interface')"
            >
              <option v-for="opt in sideB.endpointOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
            </optgroup>
            <template v-else>
              <option v-for="opt in sideB.endpointOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
            </template>
          </select>
        </div>
      </div>
      <!-- 链路属性 -->
      <div class="form-row">
        <div class="form-group">
          <label for="cable-link-link-type">{{ t("cable_link.link_type") }}</label>
          <select id="cable-link-link-type" v-model="form.linkType">
            <option v-for="opt in LINK_TYPE_OPTIONS" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="cable-link-cable-label">{{ t("cable_link.cable_label") }}</label>
          <input id="cable-link-cable-label" v-model="form.cableLabel" type="text" autocomplete="off" />
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="cable-link-length">{{ t("cable_link.length_m") }}</label>
          <input id="cable-link-length" v-model="form.length" type="number" step="0.1" min="0" autocomplete="off" />
        </div>
        <div class="form-group">
          <label for="cable-link-tested">{{ t("cable_link.tested") }}</label>
          <select id="cable-link-tested" v-model="form.tested">
            <option value="0">{{ t("cable_link.tested_no") }}</option>
            <option value="1">{{ t("cable_link.tested_yes") }}</option>
          </select>
        </div>
      </div>
    </form>
    <template #footer>
      <button
        type="button"
        class="btn btn-primary"
        :disabled="saveMutation.isPending.value"
        @click="saveMutation.mutate()"
      >
        {{ saveMutation.isPending.value ? t("common.saving") : t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
