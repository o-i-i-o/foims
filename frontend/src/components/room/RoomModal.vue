<script setup>
/**
 * 房间新增/编辑弹窗（移植自旧版 modals/room/room-modal.html
 * + room.js openRoomModal/submitRoomForm）：
 * - 基础字段：组织（树形缩进下拉）/ 房型 / 名称 / 电源功率（W|kW 单位换算）/ 描述；
 * - 网段配置行：区域 + 网段级联下拉，行间去重，至少保留一行；
 * - 子项动态行按房型分区：办公类（办公室/大厅/前台）仅工位、
 *   机房类（机房/弱电井）仅机柜、"其他"两类均可，切换房型清空列表；
 * - 信息点动态行：所有房型通用，本地校验非空 + 弹窗内不重复
 *   （跨房间重名由后端兜底）。
 * 提交链：POST/PUT rooms → PUT rooms/{id}/children → PUT rooms/{id}/net-outlets，
 * 保存期间禁用提交按钮防重复提交。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client.js";
import { showToast } from "../../composables/useToast.js";
import { readPowerWattsInput } from "../../utils/power.js";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import NetworkConfigRow from "./NetworkConfigRow.vue";

const props = defineProps({
  // null = 新增；对象 = 编辑回显（GET /rooms/{id}/brief 轻量数据）
  room: { type: Object, default: null }
});

const emit = defineEmits(["close", "saved"]);

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 基础表单 ====================

const form = reactive({
  name: props.room?.name ?? "",
  // 后端按大写存储房型，表单统一用小写
  roomType: (props.room?.room_type || "OFFICE").toLowerCase(),
  orgId: props.room?.org_id ?? "",
  totalPower: props.room?.total_power_watts ?? "",
  powerUnit: "W",
  description: props.room?.description ?? ""
});

const ROOM_TYPE_OPTIONS = ["office", "lobby", "reception", "data_center", "telecom_closet", "other"];

// 房型 i18n 键后缀映射（data_center/telecom_closet 的键名与枚举值不同）
const ROOM_TYPE_KEY_SUFFIX = {
  office: "office",
  lobby: "lobby",
  reception: "reception",
  data_center: "datacenter",
  telecom_closet: "telecom_closet",
  other: "other"
};

// ==================== 网段配置行 ====================

let nextRowKey = 1;

const networkConfigs = reactive(
  (props.room?.networks ?? []).map((network) => ({
    key: nextRowKey++,
    regionId: network.network_region_id || "",
    networkId: network.id
  }))
);

if (networkConfigs.length === 0) {
  networkConfigs.push({ key: nextRowKey++, regionId: "", networkId: "" });
}

/** 其他行已选网段 id：当前行选项过滤用（本行自身选择不参与过滤） */
function excludedNetworkIds(currentKey) {
  return networkConfigs
    .filter((row) => row.key !== currentKey && row.networkId)
    .map((row) => row.networkId);
}

function addNetworkConfig() {
  networkConfigs.push({ key: nextRowKey++, regionId: "", networkId: "" });
}

function removeNetworkConfig(index) {
  // 至少保留一行网段配置（旧版 keep_one_network 语义）
  if (networkConfigs.length <= 1) {
    showToast(t("room.keep_one_network"), "warning");
    return;
  }
  networkConfigs.splice(index, 1);
}

// ==================== 房型分区与子项动态行 ====================

// 办公类仅工位；机房类仅机柜；"其他"两类均可管理
const isOfficeRoom = computed(() => ["office", "lobby", "reception"].includes(form.roomType));
const isCabinetRoom = computed(() => ["data_center", "telecom_closet"].includes(form.roomType));
const isMixedRoom = computed(() => form.roomType === "other");
const showWorkstations = computed(() => isOfficeRoom.value || isMixedRoom.value);
const showCabinets = computed(() => isCabinetRoom.value || isMixedRoom.value);

const workstations = reactive(
  (props.room?.workstations ?? []).map((ws) => ({
    key: nextRowKey++,
    id: ws.id,
    name: ws.name || "",
    managerEmployeeId: ws.manager_employee_id || ""
  }))
);

const childCabinets = reactive(
  (props.room?.cabinets ?? []).map((cab) => ({
    key: nextRowKey++,
    id: cab.id,
    name: cab.name || "",
    capacity: cab.capacity || 42
  }))
);

// 切换房型：清空工位与机柜列表（"其他"切换到任一类型/反向均不保留）
function onRoomTypeChange() {
  workstations.splice(0);
  childCabinets.splice(0);
}

function addWorkstation() {
  workstations.push({ key: nextRowKey++, id: null, name: "", managerEmployeeId: "" });
}

function addChildCabinet() {
  childCabinets.push({ key: nextRowKey++, id: null, name: "", capacity: 42 });
}

// ==================== 信息点动态行 ====================

const netOutlets = reactive(
  (props.room?.net_outlets ?? []).map((outlet) => ({
    key: nextRowKey++,
    id: outlet.id,
    name: outlet.name || ""
  }))
);

function addNetOutlet() {
  netOutlets.push({ key: nextRowKey++, id: null, name: "" });
}

// ==================== 下拉选项（组织树 / 组织员工） ====================

// 组织选项：树形按深度缩进展示（旧版 loadOrgsForSelect 语义）
const orgsQuery = useQuery({
  queryKey: ["organizations", "tree"],
  queryFn: async () => {
    const result = await apiGet("/api/resources/organizations/tree");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const orgOptions = computed(() => {
  const flat = [];
  const walk = (nodes, depth) => {
    for (const node of nodes) {
      flat.push({ id: node.id, name: node.name, orgType: node.org_type, depth });
      if (Array.isArray(node.children) && node.children.length > 0) {
        walk(node.children, depth + 1);
      }
    }
  };
  walk(Array.isArray(orgsQuery.data.value) ? orgsQuery.data.value : [], 0);
  return flat;
});

// 管理人候选：当前房间所属组织下的员工（组织为空时不发请求）
const employeesQuery = useQuery({
  queryKey: computed(() => ["room-employees", form.orgId]),
  enabled: computed(() => Boolean(form.orgId)),
  queryFn: async () => {
    const result = await apiGet(`/api/resources/employees?org_id=${form.orgId}`);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const employeeOptions = computed(() =>
  Array.isArray(employeesQuery.data.value) ? employeesQuery.data.value : []
);

// ==================== 提交 ====================

const saving = ref(false);

/** 信息点本地校验：名称非空且弹窗内不重复（全局唯一由后端兜底） */
function validateNetOutletsLocal() {
  const seen = new Set();
  for (const outlet of netOutlets) {
    if (!outlet.name.trim()) {
      return t("net_outlet.name_required");
    }
    if (seen.has(outlet.name.trim())) {
      return t("net_outlet.name_duplicate", { name: outlet.name.trim() });
    }
    seen.add(outlet.name.trim());
  }
  return null;
}

/** 子项校验：按房型检查工位（名称+管理人）与机柜（名称+容量 1-48） */
function validateChildren() {
  if (showWorkstations.value) {
    for (const ws of workstations) {
      if (!ws.name.trim()) {
        return t("room.child_name_required");
      }
      if (!ws.managerEmployeeId) {
        return t("workstation.manager_placeholder");
      }
    }
  }
  if (showCabinets.value) {
    for (const cab of childCabinets) {
      if (!cab.name.trim()) {
        return t("room.child_name_required");
      }
      if (!Number.isInteger(cab.capacity) || cab.capacity < 1 || cab.capacity > 48) {
        return t("room.child_capacity_invalid");
      }
    }
  }
  return null;
}

const saveMutation = useMutation({
  mutationFn: async () => {
    // 前置校验链（与旧版 submitRoomForm 顺序一致）
    if (!form.name.trim()) {
      throw new Error(t("room.name_required"));
    }
    if (!form.roomType) {
      throw new Error(t("room.type_required"));
    }
    if (networkConfigs.some((row) => !row.networkId)) {
      throw new Error(t("room.network_all_required"));
    }
    const subnetIds = [...new Set(networkConfigs.map((row) => row.networkId))];
    if (subnetIds.length === 0) {
      throw new Error(t("room.network_required"));
    }
    const totalPowerWatts = readPowerWattsInput(form.totalPower, form.powerUnit);
    if (Number.isNaN(totalPowerWatts)) {
      throw new Error(t("room.total_power_invalid"));
    }
    const childrenError = validateChildren();
    if (childrenError) {
      throw new Error(childrenError);
    }
    const outletsError = validateNetOutletsLocal();
    if (outletsError) {
      throw new Error(outletsError);
    }

    const roomData = {
      name: form.name.trim(),
      // 后端统一按大写存储房型
      room_type: form.roomType.toUpperCase(),
      org_id: form.orgId || null,
      total_power_watts: totalPowerWatts,
      subnet_ids: subnetIds,
      description: form.description.trim() || null
    };

    let roomId = props.room?.id;
    if (roomId) {
      const result = await apiPut(`/api/resources/rooms/${roomId}`, roomData);
      if (!result.success) {
        throw new Error(result.message || t("room.save_failed"));
      }
    } else {
      const result = await apiPost("/api/resources/rooms", roomData);
      if (!result.success) {
        throw new Error(result.message || t("room.save_failed"));
      }
      roomId = result.data?.id;
      if (!roomId) {
        throw new Error(t("room.no_id_returned"));
      }
    }

    // 同步子项（工位/机柜，按房型构造）
    const childrenData = showCabinets.value
      ? {
          cabinets: childCabinets.map((cab) => ({
            id: cab.id,
            name: cab.name.trim(),
            capacity: cab.capacity
          })),
          ...(isMixedRoom.value
            ? {
                workstations: workstations.map((ws) => ({
                  id: ws.id,
                  name: ws.name.trim(),
                  manager_employee_id: ws.managerEmployeeId
                }))
              }
            : {})
        }
      : {
          workstations: workstations.map((ws) => ({
            id: ws.id,
            name: ws.name.trim(),
            manager_employee_id: ws.managerEmployeeId
          }))
        };
    const syncResult = await apiPut(`/api/resources/rooms/${roomId}/children`, childrenData);
    if (!syncResult.success) {
      throw new Error(syncResult.message || t("room.children_save_failed"));
    }

    // 同步信息点（所有房型通用）
    const noSyncResult = await apiPut(`/api/resources/rooms/${roomId}/net-outlets`, {
      net_outlets: netOutlets.map((outlet) => ({
        id: outlet.id,
        name: outlet.name.trim()
      }))
    });
    if (!noSyncResult.success) {
      throw new Error(noSyncResult.message || t("room.net_outlets_save_failed"));
    }

    return roomId;
  },
  onSuccess: () => {
    queryClient.invalidateQueries({ queryKey: ["rooms"] });
    showToast(t("room.save_success"), "success");
    emit("saved");
    emit("close");
  },
  onError: (error) => {
    showToast(`${t("common.operation_failed")}: ${error.message}`, "error");
  },
  onSettled: () => {
    saving.value = false;
  }
});

function submit() {
  if (saving.value) {
    return;
  }
  saving.value = true;
  saveMutation.mutate();
}
</script>

<template>
  <ModalShell :title="props.room ? t('room.edit') : t('room.add')" @close="emit('close')">
    <form @submit.prevent="submit">
      <div class="form-row">
        <div class="form-group">
          <label for="room-org-id">{{ t("organization.parent_org") }}</label>
          <select id="room-org-id" v-model="form.orgId">
            <option value="">{{ t("organization.select_org") }}</option>
            <option v-for="org in orgOptions" :key="org.id" :value="org.id">
              {{ "\u00A0".repeat(4 * org.depth) + org.name }} ({{ org.orgType }})
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="room-type">
            <span>{{ t("room.type") }}</span><span class="required">*</span>
          </label>
          <select id="room-type" v-model="form.roomType" required @change="onRoomTypeChange">
            <option v-for="type in ROOM_TYPE_OPTIONS" :key="type" :value="type">
              {{ t(`room.type_${ROOM_TYPE_KEY_SUFFIX[type]}`) }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="room-name">
            <span>{{ t("room.name") }}</span><span class="required">*</span>
          </label>
          <input id="room-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
        <div class="form-group power-inline">
          <label for="room-total-power">{{ t("room.total_power") }}</label>
          <div class="power-input-row">
            <input
              id="room-total-power"
              v-model="form.totalPower"
              type="number"
              min="0"
              step="any"
              autocomplete="off"
            >
            <select v-model="form.powerUnit" class="form-control" aria-label="Unit">
              <option value="W">W</option>
              <option value="kW">kW</option>
            </select>
          </div>
        </div>
      </div>

      <!-- 网段配置：标题右侧外部添加按钮，至少保留一行 -->
      <div class="form-group network-config-wrapper">
        <div class="list-header">
          <span class="list-title">{{ t("network.network_config") }}</span>
          <button
            type="button"
            class="icon-btn btn-success list-add-btn"
            :data-tooltip="t('network.add_network_config')"
            :aria-label="t('network.add_network_config')"
            @click="addNetworkConfig"
          >
            <NavIcon name="plus" />
          </button>
        </div>
        <div class="dyn-list">
          <div class="dyn-columns-header" aria-hidden="true">
            <span>{{ t("network.region") }}</span>
            <span>{{ t("network.subnet") }}</span>
            <span class="dyn-columns-actions" />
          </div>
          <NetworkConfigRow
            v-for="(row, index) in networkConfigs"
            :key="row.key"
            v-model="networkConfigs[index]"
            :excluded-ids="excludedNetworkIds(row.key)"
            @remove="removeNetworkConfig(index)"
          />
        </div>
      </div>

      <!-- 工位/机柜动态区域："其他"类型房间两者皆可管理 -->
      <div v-show="showWorkstations" class="form-group">
        <div class="list-header">
          <span class="list-title">{{ t("room.workstations") }}</span>
          <button
            type="button"
            class="icon-btn btn-success list-add-btn"
            :data-tooltip="t('room.add_workstation')"
            :aria-label="t('room.add_workstation')"
            @click="addWorkstation"
          >
            <NavIcon name="plus" />
          </button>
        </div>
        <div class="dyn-list">
          <div class="dyn-columns-header" aria-hidden="true">
            <span>{{ t("workstation.name") }}</span>
            <span>{{ t("workstation.manager") }}</span>
            <span class="dyn-columns-actions" />
          </div>
          <div v-if="workstations.length === 0" class="text-center hint-text">
            {{ t("common.no_data") }}
          </div>
          <template v-else>
            <div v-for="(ws, index) in workstations" :key="ws.key" class="room-child-item dyn-list-item">
              <div class="form-row">
                <div class="form-group">
                  <input
                    v-model="ws.name"
                    type="text"
                    :placeholder="t('workstation.name')"
                    autocomplete="off"
                  >
                </div>
                <div class="form-group">
                  <select v-model="ws.managerEmployeeId" autocomplete="off">
                    <option value="" disabled>{{ t("workstation.manager_placeholder") }}</option>
                    <option v-for="emp in employeeOptions" :key="emp.id" :value="emp.id">
                      {{ emp.name }}
                    </option>
                  </select>
                </div>
                <div class="form-group room-children-actions">
                  <button
                    type="button"
                    class="icon-btn btn-danger"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="workstations.splice(index, 1)"
                  >
                    <NavIcon name="trash" />
                  </button>
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>

      <div v-show="showCabinets" class="form-group">
        <div class="list-header">
          <span class="list-title">{{ t("room.cabinets") }}</span>
          <button
            type="button"
            class="icon-btn btn-success list-add-btn"
            :data-tooltip="t('cabinet.add_cabinet')"
            :aria-label="t('cabinet.add_cabinet')"
            @click="addChildCabinet"
          >
            <NavIcon name="plus" />
          </button>
        </div>
        <div class="dyn-list">
          <div class="dyn-columns-header" aria-hidden="true">
            <span>{{ t("cabinet.name") }}</span>
            <span>{{ t("cabinet.capacity") }}</span>
            <span class="dyn-columns-actions" />
          </div>
          <div v-if="childCabinets.length === 0" class="text-center hint-text">
            {{ t("common.no_data") }}
          </div>
          <template v-else>
            <div v-for="(cab, index) in childCabinets" :key="cab.key" class="room-child-item dyn-list-item">
              <div class="form-row">
                <div class="form-group">
                  <input
                    v-model="cab.name"
                    type="text"
                    :placeholder="t('cabinet.name')"
                    autocomplete="off"
                  >
                </div>
                <div class="form-group">
                  <input
                    v-model.number="cab.capacity"
                    type="number"
                    min="1"
                    max="48"
                    :placeholder="t('cabinet.capacity')"
                  >
                </div>
                <div class="form-group room-children-actions">
                  <button
                    type="button"
                    class="icon-btn btn-danger"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="childCabinets.splice(index, 1)"
                  >
                    <NavIcon name="trash" />
                  </button>
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>

      <!-- 信息点：所有房型通用 -->
      <div class="form-group">
        <div class="list-header">
          <span class="list-title">{{ t("room.net_outlets") }}</span>
          <button
            type="button"
            class="icon-btn btn-success list-add-btn"
            :data-tooltip="t('room.add_net_outlet')"
            :aria-label="t('room.add_net_outlet')"
            @click="addNetOutlet"
          >
            <NavIcon name="plus" />
          </button>
        </div>
        <div class="dyn-list">
          <div v-if="netOutlets.length === 0" class="text-center hint-text">
            {{ t("common.no_data") }}
          </div>
          <template v-else>
            <div v-for="(outlet, index) in netOutlets" :key="outlet.key" class="room-net-outlet-item dyn-list-item">
              <div class="form-row">
                <div class="form-group">
                  <input
                    v-model="outlet.name"
                    type="text"
                    :placeholder="t('net_outlet.name')"
                    autocomplete="off"
                  >
                </div>
                <div class="form-group room-net-outlets-actions">
                  <button
                    type="button"
                    class="icon-btn btn-danger"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="netOutlets.splice(index, 1)"
                  >
                    <NavIcon name="trash" />
                  </button>
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>

      <div class="form-group form-group-block">
        <textarea
          v-model="form.description"
          :aria-label="t('common.description')"
          :placeholder="t('common.description_placeholder')"
        />
      </div>
    </form>

    <template #footer>
      <button type="button" class="btn-primary" :disabled="saving" @click="submit">
        {{ saving ? t("common.saving") : t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
