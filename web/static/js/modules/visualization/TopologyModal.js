import { TopologyDataManager } from "./TopologyDataManager.js";
import { t } from "../../utils/i18n.js";

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
  }

  open(deviceId, deviceName) {
    this.currentDeviceId = deviceId;
    this.currentDeviceName = deviceName || deviceId;
    this.panelVisibility = { ports: true, macs: false, lldp: false };
    this.activePanel = "ports";
    this._render();
    this._loadData();
  }

  close() {
    if (this.overlay) {
      this.overlay.remove();
      this.overlay = null;
    }
    if (this.modal) {
      this.modal.remove();
      this.modal = null;
    }
    this.currentDeviceId = null;
  }

  isOpen() {
    return this.currentDeviceId !== null;
  }

  _render() {
    this.close();

    this.overlay = document.createElement("div");
    this.overlay.className = "topology-detail-overlay";
    this.overlay.addEventListener("click", () => this.close());

    this.modal = document.createElement("div");
    this.modal.className = "topology-detail-modal";

    this.modal.innerHTML = `
      <div class="topology-detail-header">
        <h3>${this._escapeHtml(this.currentDeviceName)}</h3>
        <button class="topology-detail-close" aria-label="Close">&times;</button>
      </div>
      <div class="topology-detail-tabs">
        <button class="toggle-ports active" data-panel="ports">${t("visualization.show_ports")}</button>
        <button class="toggle-macs" data-panel="macs">${t("visualization.show_macs")}</button>
        <button class="toggle-lldp" data-panel="lldp">${t("visualization.show_lldp")}</button>
      </div>
      <div class="topology-detail-body">
        <div class="topology-panel ports-panel"></div>
        <div class="topology-panel macs-panel" style="display:none"></div>
        <div class="topology-panel lldp-panel" style="display:none"></div>
      </div>
      <div class="topology-detail-footer">
        <button class="btn btn-danger btn-sm topology-remove-btn">${t("visualization.remove_from_topology")}</button>
      </div>
    `;

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

    document.body.appendChild(this.overlay);
    document.body.appendChild(this.modal);
  }

  _togglePanel(panel) {
    const visiblePanels = Object.entries(this.panelVisibility).filter(([, v]) => v);
    if (this.panelVisibility[panel] && visiblePanels.length <= 1) return;

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
        el.style.display = this.panelVisibility[panel] ? "" : "none";
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
    if (!this.currentDeviceId) return;
    await Promise.all([
      this._loadPanelData("ports"),
      this.panelVisibility.macs ? this._loadPanelData("macs") : Promise.resolve(),
      this.panelVisibility.lldp ? this._loadPanelData("lldp") : Promise.resolve()
    ]);
  }

  async _loadPanelData(panel) {
    if (!this.modal || !this.currentDeviceId) return;

    const container = this.modal.querySelector(`.${panel}-panel`);
    if (!container) return;

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
              <td>${this._escapeHtml(p.port_number || "")}</td>
              <td>${this._escapeHtml(p.port_name || "")}</td>
              <td>${this._escapeHtml(p.admin_status || p.oper_status || "")}</td>
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
              <td>${this._escapeHtml(m.mac_address || "")}</td>
              <td>${this._escapeHtml(m.vlan_id != null ? String(m.vlan_id) : "")}</td>
              <td>${this._escapeHtml(m.port_name || m.interface_name || "")}</td>
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
              <td>${this._escapeHtml(n.local_port || n.local_interface || "")}</td>
              <td>${this._escapeHtml(n.system_name || n.neighbor_name || "")}</td>
              <td>${this._escapeHtml(n.neighbor_port || n.remote_interface || "")}</td>
            </tr>`
            )
            .join("")}
        </tbody>
      </table>`;
  }

  _escapeHtml(str) {
    if (!str) return "";
    const div = document.createElement("div");
    div.textContent = str;
    return div.innerHTML;
  }
}
