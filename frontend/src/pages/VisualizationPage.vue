<script setup lang="ts">
/**
 * 可视化页（移植自旧版 main.html visualization 分区 + visualizationManager.js）：
 * 画布生态（11 个 visualization JS）逐字迁移到 modules/visualization/，本页只提供
 * DOM 壳与挂载时序——onMounted 调 initVisualization()，并把画布三个编辑回调
 * （工位/机柜/机位）经 legacyBridge.vueModals 注入为 Vue 弹窗实现。
 *
 * 组件名 VisualizationPage 供 MainShell 的 <KeepAlive include> 缓存：旧版分区
 * DOM 常驻不销毁（display 切换），画布模块级单例、elementCache 与 document 级
 * 监听均依赖该语义，离开页面后返回不重建。
 */
import { onMounted, reactive } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet } from "../api/client";
import { handleError, showToast, vueModals } from "../utils/legacyBridge.js";
import { initVisualization } from "../modules/visualization/visualizationManager.js";
import WorkstationModal from "../components/workstation/WorkstationModal.vue";
import CabinetModal from "../components/cabinet/CabinetModal.vue";
import PositionModal from "../components/cabinet/PositionModal.vue";
import type { Workstation } from "../types/api/generated/Workstation";
import type { CabinetWithNetworks } from "../types/api/generated/CabinetWithNetworks";
import type { CabinetPosition } from "../types/api/generated/CabinetPosition";

defineOptions({ name: "VisualizationPage" });

const { t } = useI18n();

// ==================== Vue 编辑弹窗（画布回调注入点） ====================

/** 画布当前坐标（visualizationManager 回调传入，回填坐标输入框） */
type CanvasPosition = { x: number; y: number } | null;

// 工位弹窗：workstation 为 GET 详情回显对象，position 为画布当前坐标
const workstationModal = reactive({
  open: false,
  workstation: null as Workstation | null,
  position: null as CanvasPosition
});
// 机柜弹窗：与 ResourcesPage.editCabinet 同模式（详情含机位/配线架子项）
const cabinetModal = reactive({ open: false, cabinet: null as CabinetWithNetworks | null });
// 机位弹窗：position 为 GET 详情回显对象
const positionModal = reactive({ open: false, position: null as CabinetPosition | null });

/** 画布工位编辑回调 → GET 详情后打开 Vue 工位弹窗（旧版 editWorkstation 语义） */
async function openWorkstationEditor(id: string, position: CanvasPosition = null): Promise<void> {
  try {
    const result = await apiGet<Workstation>(`/api/resources/workstations/${id}`);
    if (result.success) {
      workstationModal.workstation = result.data;
      workstationModal.position = position;
      workstationModal.open = true;
    } else {
      showToast(`${t("workstation.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("workstation.load_failed"));
  }
}

/** 画布机柜编辑回调 → GET 详情后打开 Vue 机柜弹窗（旧版 editCabinet 语义） */
async function openCabinetEditor(id: string): Promise<void> {
  try {
    const result = await apiGet<CabinetWithNetworks>(`/api/resources/cabinets/${id}`);
    if (result.success && result.data) {
      cabinetModal.cabinet = result.data;
      cabinetModal.open = true;
    } else {
      showToast(`${t("cabinet.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("cabinet.load_failed"));
  }
}

/** 画布机位编辑回调 → GET 详情后打开 Vue 机位弹窗（旧版 editCabinetPosition 语义） */
async function openPositionEditor(id: string): Promise<void> {
  try {
    const result = await apiGet<CabinetPosition>(`/api/resources/positions/${id}`);
    if (result.success) {
      positionModal.position = result.data;
      positionModal.open = true;
    } else {
      showToast(`${t("cabinet_position.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("cabinet_position.load_failed"));
  }
}

onMounted(() => {
  // 注入 Vue 弹窗实现（赋值幂等，KeepAlive 下 onMounted 仅执行一次）。
  // legacyBridge.js 为遗留 JS 模块（checkJs 关闭）无类型，holder 属性按
  // 画布回调契约断言后注入
  const modalBridge = vueModals as {
    editWorkstation: (id: string, position: CanvasPosition) => void;
    editCabinet: (id: string) => void;
    editCabinetPosition: (id: string) => void;
  };
  modalBridge.editWorkstation = openWorkstationEditor;
  modalBridge.editCabinet = openCabinetEditor;
  modalBridge.editCabinetPosition = openPositionEditor;

  initVisualization();
});
</script>

<template>
  <section id="visualization" class="content-section active visualization-page" :aria-label="t('nav.visualization')">
    <div class="tabs" role="tablist">
      <button class="tab-btn active" data-tab="workstation-visualization" role="tab" aria-selected="true" aria-controls="workstation-visualization">
        {{ t("visualization.workstation") }}
      </button>
      <button class="tab-btn" data-tab="cabinet-visualization" role="tab" aria-selected="false" aria-controls="cabinet-visualization">
        {{ t("visualization.cabinet") }}
      </button>
      <button class="tab-btn" data-tab="global-visualization" role="tab" aria-selected="false" aria-controls="global-visualization">
        {{ t("visualization.device") }}
      </button>
    </div>

    <div id="workstation-visualization" class="tab-content active" role="tabpanel">
      <div class="toolbar">
        <select id="viz-org-select" class="form-control" aria-label="Select organization"></select>
        <select id="viz-room-type" class="form-control" aria-label="Select room type">
          <option value="">{{ t("viz.all_room_types") }}</option>
          <option value="office">{{ t("room.type_office") }}</option>
          <option value="lobby">{{ t("room.type_lobby") }}</option>
          <option value="reception">{{ t("room.type_reception") }}</option>
          <option value="other">{{ t("room.type_other") }}</option>
        </select>
        <select id="room-select" class="form-control" aria-label="Select room"></select>
        <button id="auto-draw-workstations" class="btn btn-primary btn-sm">{{ t("visualization.auto_draw") }}</button>
        <button id="save-workstation-layout" class="btn btn-secondary btn-sm">{{ t("visualization.save_layout") }}</button>
        <button id="delete-workstation-layout" class="btn btn-danger btn-sm">{{ t("visualization.delete_layout") }}</button>
      </div>
      <div id="workstation-visualization-container" class="visualization-canvas visualization-canvas-scroll" role="img" :aria-label="t('visualization.workstation')"></div>
    </div>

    <div id="cabinet-visualization" class="tab-content" role="tabpanel">
      <div class="toolbar">
        <select id="cabinet-viz-org-select" class="form-control" aria-label="Select organization for cabinets"></select>
        <select id="cabinet-room-type" class="form-control" aria-label="Select room type for cabinets">
          <option value="">{{ t("viz.all_room_types") }}</option>
          <option value="data_center">{{ t("room.type_datacenter") }}</option>
          <option value="telecom_closet">{{ t("room.type_telecom_closet") }}</option>
          <option value="other">{{ t("room.type_other") }}</option>
        </select>
        <select id="cabinet-room-select" class="form-control" aria-label="Select room for cabinets"></select>
        <button id="auto-draw-cabinet-positions" class="btn btn-primary btn-sm">{{ t("visualization.auto_draw") }}</button>
        <button id="delete-cabinet-layout" class="btn btn-danger btn-sm">{{ t("visualization.delete_layout") }}</button>
        <button id="save-cabinet-layout" class="btn btn-secondary btn-sm">{{ t("visualization.save_layout") }}</button>
      </div>
      <div id="cabinet-visualization-container" class="visualization-canvas visualization-canvas-scroll" role="img" :aria-label="t('visualization.cabinet')"></div>
    </div>

    <div id="global-visualization" class="tab-content" role="tabpanel">
      <div class="toolbar">
        <select id="topology-org-select" class="form-control" aria-label="Select organization for topology"></select>
        <button id="toggle-connection-mode" class="btn btn-warning btn-sm">{{ t("visualization.connection_mode") }}</button>
        <button id="open-topology-connection-modal" class="btn btn-primary btn-sm">{{ t("viz.create_connection") }}</button>
        <button id="topology-auto-discover" class="btn btn-info btn-sm">{{ t("visualization.auto_discover") }}</button>
        <button id="topology-auto-layout" class="btn btn-primary btn-sm">{{ t("visualization.auto_layout") }}</button>
        <button id="save-topology-layout" class="btn btn-secondary btn-sm">{{ t("visualization.save_layout") }}</button>
        <div class="zoom-controls">
          <button
            id="topology-zoom-out"
            type="button"
            class="btn btn-secondary btn-sm"
            :aria-label="t('viz.zoom_out')"
            :data-tooltip="t('viz.zoom_out')"
          >&minus;</button>
          <select id="topology-zoom-level" class="form-control zoom-select" :aria-label="t('viz.zoom_level')">
            <option value="0.25">25%</option>
            <option value="0.5">50%</option>
            <option value="0.75">75%</option>
            <option value="1" selected>100%</option>
            <option value="1.5">150%</option>
            <option value="2">200%</option>
            <option value="3">300%</option>
            <option value="fit">{{ t("viz.zoom_fit") }}</option>
          </select>
          <button
            id="topology-zoom-in"
            type="button"
            class="btn btn-secondary btn-sm"
            :aria-label="t('viz.zoom_in')"
            :data-tooltip="t('viz.zoom_in')"
          >+</button>
        </div>
      </div>
      <div id="global-visualization-container" class="visualization-canvas visualization-canvas-fixed" role="img" :aria-label="t('visualization.device')"></div>
    </div>

    <!-- Vue 编辑弹窗宿主：画布回调经 legacyBridge.vueModals 打开 -->
    <WorkstationModal
      v-if="workstationModal.open"
      :workstation="workstationModal.workstation"
      :position="workstationModal.position"
      @close="workstationModal.open = false"
    />
    <CabinetModal v-if="cabinetModal.open" :cabinet="cabinetModal.cabinet" @close="cabinetModal.open = false" />
    <PositionModal v-if="positionModal.open" :position="positionModal.position" @close="positionModal.open = false" />
  </section>
</template>
