import { apiGet, apiPost, apiDelete } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";
import { t } from "../../utils/i18n.js";

export class TopologyDataManager {
  constructor() {
    this.apiGet = apiGet;
    this.apiPost = apiPost;
    this.apiDelete = apiDelete;
    this.showToast = showToast;
  }

  /** 数据获取失败时的统一提示（请求异常或业务失败） */
  _notifyLoadFailure(error, what) {
    console.error(`${what}失败:`, error);
    this.showToast(t("viz.data_load_failed"), "error");
  }

  async fetchTopologyNodes() {
    try {
      const result = await this.apiGet("/api/resources/topology/nodes");
      if (result.success) {
        return Array.isArray(result.data) ? result.data : [];
      }
      this._notifyLoadFailure(result.message, "获取拓扑节点");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取拓扑节点");
      return [];
    }
  }

  /**
   * 保存拓扑节点坐标（按 device_id upsert）。
   * @param {Array<{device_id: string, x: number, y: number, width: number, height: number}>} nodes
   * @param {Object} [options]
   * @param {boolean} [options.silent] 静默模式：成功不提示（拖拽自动保存使用），失败仍提示
   */
  async saveTopologyNodes(nodes, { silent = false } = {}) {
    try {
      const result = await this.apiPost("/api/resources/topology/nodes", { nodes });
      if (result.success) {
        if (!silent) {
          this.showToast(t("viz.node_save_success"), "success");
        }
        return true;
      }
      this.showToast(`${t("viz.node_save_failed")}: ${result.message}`, "error");
      return false;
    } catch (error) {
      console.error("保存拓扑节点失败:", error);
      this.showToast(t("viz.node_save_failed"), "error");
      return false;
    }
  }

  async deleteTopologyNode(deviceId) {
    try {
      const result = await this.apiDelete(`/api/resources/topology/nodes/${deviceId}`);
      if (result.success) {
        this.showToast(t("viz.device_removed"), "success");
        return true;
      }
      this.showToast(`${t("viz.delete_failed")}: ${result.message}`, "error");
      return false;
    } catch (error) {
      console.error("删除拓扑节点失败:", error);
      this.showToast(t("viz.node_delete_failed"), "error");
      return false;
    }
  }

  async fetchTopologyConnections() {
    try {
      const result = await this.apiGet("/api/resources/topology/connections");
      if (result.success) {
        return Array.isArray(result.data) ? result.data : [];
      }
      this._notifyLoadFailure(result.message, "获取拓扑连线");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取拓扑连线");
      return [];
    }
  }

  async createConnection(data) {
    try {
      const result = await this.apiPost("/api/resources/topology/connections", data);
      if (result.success) {
        this.showToast(t("viz.link_create_success"), "success");
        return result.data;
      }
      this.showToast(`${t("viz.link_create_failed")}: ${result.message}`, "error");
      return null;
    } catch (error) {
      console.error("创建连线失败:", error);
      this.showToast(t("viz.link_create_failed"), "error");
      return null;
    }
  }

  async deleteConnection(id) {
    try {
      const result = await this.apiDelete(`/api/resources/topology/connections/${id}`);
      if (result.success) {
        this.showToast(t("viz.link_delete_success"), "success");
        return true;
      }
      this.showToast(`${t("viz.link_delete_failed")}: ${result.message}`, "error");
      return false;
    } catch (error) {
      console.error("删除连线失败:", error);
      this.showToast(t("viz.link_delete_failed"), "error");
      return false;
    }
  }

  async fetchDevicePorts(deviceId) {
    try {
      const result = await this.apiGet(
        `/api/resources/devices/${deviceId}/device-ports?page_size=200`
      );
      if (result.success) {
        // 后端分页响应形状固定为 items（paged_response）
        return result.data?.items ?? [];
      }
      this._notifyLoadFailure(result.message, "获取设备端口");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取设备端口");
      return [];
    }
  }

  async fetchDeviceMacs(deviceId) {
    try {
      const result = await this.apiGet(`/api/resources/devices/${deviceId}/macs`);
      if (result.success) {
        return result.data?.items ?? [];
      }
      this._notifyLoadFailure(result.message, "获取MAC表");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取MAC表");
      return [];
    }
  }

  async fetchDeviceLldp(deviceId) {
    try {
      const result = await this.apiGet(`/api/resources/devices/${deviceId}/lldp-neighbors`);
      if (result.success) {
        return result.data?.items ?? [];
      }
      this._notifyLoadFailure(result.message, "获取LLDP邻居");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取LLDP邻居");
      return [];
    }
  }

  async fetchAllDevices() {
    try {
      const result = await this.apiGet("/api/resources/devices?page_size=1000");
      if (result.success) {
        return result.data?.items ?? [];
      }
      this._notifyLoadFailure(result.message, "获取设备列表");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取设备列表");
      return [];
    }
  }

  async autoDiscover() {
    try {
      const result = await this.apiPost("/api/resources/topology/auto-discover", {});
      if (result.success) {
        return result.data || { added_nodes: 0, added_connections: 0 };
      }
      this.showToast(`${t("viz.auto_discover_failed")}: ${result.message}`, "error");
      return null;
    } catch (error) {
      console.error("自动发现失败:", error);
      this.showToast(t("viz.auto_discover_failed"), "error");
      return null;
    }
  }
}
