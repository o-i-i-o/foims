import { SVGCore } from "./SVGCore.js";
import { SVGRenderer } from "./SVGRenderer.js";
import { SVGDataManager } from "./SVGDataManager.js";
import { t } from "../../utils/i18n.js";

export class SVGVisualization {
  constructor(containerId, type, callbacks = {}) {
    this.core = new SVGCore(containerId, type, callbacks);
    this.renderer = new SVGRenderer(this.core);
    this.dataManager = new SVGDataManager(this.core, this.renderer);

    this.elementsGroup = this.core.elementsGroup;
    this.svg = this.core.svg;
    this.container = this.core.container;
  }

  /**
   * 自动绘制工位（无已保存布局时按网格排布）。
   * @param {string} roomId 房间 id
   * @param {Object} [options]
   * @param {boolean} [options.skipReload] 调用方已通过 loadSavedLayout 确认无布局时
   *   传 true，跳过内部二次加载，避免布局/工位/IP 三类请求双发
   */
  async autoDrawWorkstations(roomId, { skipReload = false } = {}) {
    try {
      if (!skipReload) {
        const hasSavedLayout = await this.dataManager.loadSavedLayout(roomId);

        if (hasSavedLayout === null || hasSavedLayout === "error") {
          // null：已被更新的渲染取代；"error"：布局请求失败——
          // 两者都不自动排布（失败时排布落库会覆盖已保存布局）
          return;
        }
        if (hasSavedLayout) {
          this.core.currentRoomId = roomId;
          return;
        }
      }

      this.core.currentRoomId = roomId;

      // 自动排布纳入渲染代次管理：await 期间若有并发加载开启则放弃本次绘制
      const token = this.dataManager.beginWorkstationRender();

      const [workstations, ipManagers] = await Promise.all([
        this.dataManager.fetchWorkstationsByRoom(roomId),
        this.dataManager.fetchIps()
      ]);
      if (token !== this.dataManager.workstationRenderToken) {
        return;
      }

      const ipMap = new Map();
      if (Array.isArray(ipManagers)) {
        ipManagers.forEach((ipManager) => {
          if (!ipManager.workstation_id) {
            return;
          }
          const existing = ipMap.get(ipManager.workstation_id);
          if (!existing || (existing.status !== "active" && ipManager.status === "active")) {
            ipMap.set(ipManager.workstation_id, ipManager);
          }
        });
      }

      if (workstations.length === 0) {
        this.core.showToast(t("viz.no_workstation_data"), "info");
        return;
      }

      this.renderer.drawDoor();

      const gap = 20;
      const width = 160;
      const height = 160;
      const startX = 150;
      const startY = 100;

      let cols = Math.min(8, Math.max(3, Math.ceil(Math.sqrt(workstations.length))));
      if (workstations.length <= 5) {
        cols = 3;
      } else if (workstations.length <= 12) {
        cols = 4;
      } else if (workstations.length <= 24) {
        cols = 6;
      }

      const rows = Math.ceil(workstations.length / cols);

      workstations.forEach((workstation, index) => {
        const col = index % cols;
        const row = Math.floor(index / cols);

        workstation.ipManager = ipMap.get(workstation.id) || null;
        workstation.position = {
          x: startX + col * (width + gap),
          y: startY + row * (height + gap),
          width,
          height
        };

        this.renderer.drawWorkstation(workstation);
      });

      // 归属集合与 loadSavedLayout 保持同型：保存布局时仅提交本次绘制的元素
      this.dataManager.setLayoutOwnerIds(
        [...this.core.elementsGroup.querySelectorAll("[data-id]")].map((el) => el.dataset.id)
      );

      const totalWidth = startX + cols * (width + gap) + 50;
      const totalHeight = startY + rows * (height + gap) + 50;
      // viewBox 同时覆盖容器尺寸，网格背景铺满画布（与 loadSavedLayout 保持一致）
      const cw = this.container.clientWidth || 0;
      const ch = this.container.clientHeight || 0;
      this.core.setViewBox(0, 0, Math.max(1000, totalWidth, cw), Math.max(800, totalHeight, ch));
    } catch (error) {
      console.error("自动绘制工位失败:", error);
      this.core.showToast(t("viz.auto_draw_workstation_failed"), "error");
    }
  }

  async autoDrawCabinetPositions(roomId) {
    if (!roomId) {
      return;
    }

    this.core.currentRoomId = roomId;

    try {
      // 先清空画布并开启渲染代次，再做任何异步请求：
      // 机柜为 0 时也已清空画布；快速切换房间时旧响应由代次校验丢弃
      const token = this.dataManager.beginCabinetRender();

      const cabinets = await this.dataManager.fetchCabinetsByRoom(roomId);
      if (token !== this.dataManager.cabinetRenderToken) {
        return; // 已有更新的渲染在途，丢弃过期结果
      }

      if (cabinets.length === 0) {
        // 无渲染跟进:恢复完成标志,避免 saveLayout 被永久拒绝
        this.dataManager.cabinetRenderSettled = true;
        this.core.showToast(t("viz.no_cabinet_data"), "info");
        return;
      }

      // 复用与 loadSavedLayout 相同的布局与分批渲染（无保存布局，纯自动排列）
      // 返回 false 表示渲染代次已被并发操作取代，无需后续处理
      await this.dataManager.layoutAndRenderCabinets(cabinets, [], token);
    } catch (error) {
      console.error("自动绘制机位图失败:", error);
      this.core.showToast(`${t("viz.draw_positions_failed")}: ${error.message}`, "error");
    }
  }

  async loadSavedLayout(id) {
    return await this.dataManager.loadSavedLayout(id);
  }

  /** @param {{silent?: boolean}} [options] 静默保存：拖拽自动保存时不弹成功提示 */
  async saveLayout(options) {
    return await this.dataManager.saveLayout(options);
  }

  async deleteLayout() {
    return await this.dataManager.deleteLayout();
  }
}
