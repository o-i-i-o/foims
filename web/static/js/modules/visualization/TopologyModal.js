import { TopologyDataManager } from "./TopologyDataManager.js";
import { t, updatePageTranslations } from "../../utils/i18n.js";
import { fetchModalHtml } from "../../utils/modalLoader.js";
import { showToast } from "../../utils/ui.js";
import { escapeHtml } from "../../utils/helpers.js";

export class TopologyModal {
  constructor() {
    this.dataManager = new TopologyDataManager();
    this.overlay = null;
    this.modal = null;
    this.currentDeviceId = null;
    this.currentDeviceName = null;
    this.activePanel = "ports";
    this.panelVisibility = { ports: true, macs: false, lldp: false };
    this.onRemoveDevice = null;
    // 设备坐标保存回调（可视化层注入：移动节点并持久化）
    this.onSavePosition = null;
  }

  open(deviceId, deviceName, position = null) {
    this.currentDeviceId = deviceId;
    this.currentDeviceName = deviceName || deviceId;
    this.currentPosition = position;
    this.panelVisibility = { ports: true, macs: false, lldp: false };
    this.activePanel = "ports";
    // 先完成模板渲染再拉取面板数据，保证 _loadPanelData 能拿到容器节点
    this._render().then(() => this._loadData());
  }

  close() {
    this._removeDom();
    this.currentDeviceId = null;
  }

  isOpen() {
    return this.currentDeviceId !== null;
  }

  /** 仅移除浮窗 DOM；_render 重建时不能走 close()，否则会清空 currentDeviceId 导致面板数据不加载 */
  _removeDom() {
    if (this.overlay) {
      this.overlay.remove();
      this.overlay = null;
    }
    if (this.modal) {
      this.modal.remove();
      this.modal = null;
    }
  }

  // 模态框结构位于 modals/visualization/topology-detail-modal.html
  async _render() {
    this._removeDom();

    const html = await fetchModalHtml("topology-detail-modal");
    if (!html) {
      return;
    }

    this.overlay = document.createElement("div");
    this.overlay.className = "topology-detail-overlay";
    this.overlay.addEventListener("click", () => this.close());

    const wrap = document.createElement("div");
    wrap.innerHTML = html;
    this.modal = wrap.firstElementChild;

    this.modal.querySelector("#topology-detail-title").textContent = this.currentDeviceName;

    this.modal
      .querySelector(".topology-detail-close")
      .addEventListener("click", () => this.close());

    this.modal.querySelectorAll(".topology-detail-tabs button").forEach((btn) => {
      btn.addEventListener("click", () => this._togglePanel(btn.dataset.panel));
    });

    this.modal.querySelector(".topology-remove-btn").addEventListener("click", () => {
      if (this.onRemoveDevice && this.currentDeviceId) {
        this.onRemoveDevice(this.currentDeviceId);
        this.close();
      }
    });

    // 坐标配置区：回填画布当前坐标，保存时回调可视化层移动节点
    const xInput = this.modal.querySelector("#topology-detail-x");
    const yInput = this.modal.querySelector("#topology-detail-y");
    if (xInput && yInput) {
      xInput.value = Math.round(this.currentPosition?.x ?? 0);
      yInput.value = Math.round(this.currentPosition?.y ?? 0);
      const savePosBtn = this.modal.querySelector("#topology-detail-save-pos-btn");
      savePosBtn?.addEventListener("click", () => {
        const x = Number(xInput.value);
        const y = Number(yInput.value);
        if (!Number.isFinite(x) || !Number.isFinite(y) || x < 0 || y < 0) {
          showToast(t("common.check_input"), "warning");
          return;
        }
        if (this.onSavePosition && this.currentDeviceId) {
          this.onSavePosition(this.currentDeviceId, Math.round(x), Math.round(y));
          this.close();
        }
      });
    }

    document.body.appendChild(this.overlay);
    document.body.appendChild(this.modal);
    updatePageTranslations();
  }

  _togglePanel(panel) {
    const visiblePanels = Object.entries(this.panelVisibility).filter(([, v]) => v);
    if (this.panelVisibility[panel] && visiblePanels.length <= 1) {
      return;
    }

    this.panelVisibility[panel] = !this.panelVisibility[panel];

    if (this.panelVisibility[panel]) {
      this.activePanel = panel;
    } else {
      const remaining = Object.entries(this.panelVisibility).filter(([, v]) => v);
      if (remaining.length > 0) {
        this.activePanel = remaining[0][0];
      }
    }

    this._updatePanelVisibility();
    this._updateTabStates();

    if (this.panelVisibility[panel] && this.currentDeviceId) {
      this._loadPanelData(panel);
    }
  }

  _updatePanelVisibility() {
    ["ports", "macs", "lldp"].forEach((panel) => {
      const el = this.modal?.querySelector(`.${panel}-panel`);
      if (el) {
        el.classList.toggle("hidden", !this.panelVisibility[panel]);
      }
    });
  }

  _updateTabStates() {
    this.modal?.querySelectorAll(".topology-detail-tabs button").forEach((btn) => {
      const panel = btn.dataset.panel;
      btn.classList.toggle("active", this.panelVisibility[panel]);
    });
  }

  async _loadData() {
    if (!this.currentDeviceId) {
      return;
    }
    await Promise.all([
      this._loadPanelData("ports"),
      this.panelVisibility.macs ? this._loadPanelData("macs") : Promise.resolve(),
      this.panelVisibility.lldp ? this._loadPanelData("lldp") : Promise.resolve()
    ]);
  }

  async _loadPanelData(panel) {
    if (!this.modal || !this.currentDeviceId) {
      return;
    }

    const container = this.modal.querySelector(`.${panel}-panel`);
    if (!container) {
      return;
    }

    container.innerHTML = `<div class="topology-loading">${t("common.loading")}</div>`;

    try {
      let data;
      switch (panel) {
        case "ports":
          data = await this.dataManager.fetchDevicePorts(this.currentDeviceId);
          container.innerHTML = this._renderPortsTable(data);
          break;
        case "macs":
          data = await this.dataManager.fetchDeviceMacs(this.currentDeviceId);
          container.innerHTML = this._renderMacsTable(data);
          break;
        case "lldp":
          data = await this.dataManager.fetchDeviceLldp(this.currentDeviceId);
          container.innerHTML = this._renderLldpTable(data);
          break;
      }
    } catch {
      container.innerHTML = `<div class="topology-error">${t("common.load_failed")}</div>`;
    }
  }

  _renderPortsTable(ports) {
    if (!ports || ports.length === 0) {
      return `<div class="topology-empty">${t("visualization.no_ports_data")}</div>`;
    }
    return `
      <table class="topology-detail-table">
        <thead>
          <tr>
            <th>${t("visualization.port_number")}</th>
            <th>${t("visualization.port_name")}</th>
            <th>${t("visualization.port_status")}</th>
          </tr>
        </thead>
        <tbody>
          ${ports
            .map(
              (p) => `
            <tr>
              <td>${escapeHtml(p.name || "")}</td>
              <td>${escapeHtml(p.description || "")}</td>
              <td>${escapeHtml(p.status || "")}</td>
            </tr>`
            )
            .join("")}
        </tbody>
      </table>`;
  }

  _renderMacsTable(macs) {
    if (!macs || macs.length === 0) {
      return `<div class="topology-empty">${t("visualization.no_macs_data")}</div>`;
    }
    return `
      <table class="topology-detail-table">
        <thead>
          <tr>
            <th>${t("visualization.mac_address")}</th>
            <th>${t("visualization.mac_vlan")}</th>
            <th>${t("visualization.mac_port")}</th>
          </tr>
        </thead>
        <tbody>
          ${macs
            .map(
              (m) => `
            <tr>
              <td>${escapeHtml(m.mac_address || "")}</td>
              <td>${escapeHtml(m.vlan_id != null ? String(m.vlan_id) : "")}</td>
              <td>${escapeHtml(m.port_name || m.interface_name || "")}</td>
            </tr>`
            )
            .join("")}
        </tbody>
      </table>`;
  }

  _renderLldpTable(neighbors) {
    if (!neighbors || neighbors.length === 0) {
      return `<div class="topology-empty">${t("visualization.no_lldp_data")}</div>`;
    }
    return `
      <table class="topology-detail-table">
        <thead>
          <tr>
            <th>${t("visualization.lldp_port")}</th>
            <th>${t("visualization.lldp_system")}</th>
            <th>${t("visualization.lldp_neighbor_port")}</th>
          </tr>
        </thead>
        <tbody>
          ${neighbors
            .map(
              (n) => `
            <tr>
              <td>${escapeHtml(n.local_port || n.local_interface || "")}</td>
              <td>${escapeHtml(n.system_name || n.neighbor_name || "")}</td>
              <td>${escapeHtml(n.neighbor_port || n.remote_interface || "")}</td>
            </tr>`
            )
            .join("")}
        </tbody>
      </table>`;
  }
}
