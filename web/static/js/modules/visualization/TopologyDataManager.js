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
    // 全量分页拉取：单页 page_size=200 会在设备端口超 200 时静默截断
    return this._fetchAllPages(`/api/resources/devices/${deviceId}/interfaces`, "获取设备端口");
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

  /**
   * 分页拉取全量列表：每页 1000 条，最多 5 页（5000 条）。
   * 仍有后续页但已达上限时提示数据可能不完整（避免超 1000 条被静默截断）。
   * @param {string} path 不含分页参数的接口路径
   * @param {string} what 失败提示用途描述
   * @returns {Promise<Array>} 拉取到的条目（失败时为已获取的部分或空数组）
   */
  async _fetchAllPages(path, what) {
    const MAX_PAGES = 5;
    const PAGE_SIZE = 1000;
    const items = [];
    const sep = path.includes("?") ? "&" : "?";

    for (let page = 1; page <= MAX_PAGES; page++) {
      let result;
      try {
        result = await this.apiGet(`${path}${sep}page=${page}&page_size=${PAGE_SIZE}`);
      } catch (error) {
        this._notifyLoadFailure(error, what);
        break;
      }
      if (!result.success || !Array.isArray(result.data?.items)) {
        if (!result.success) {
          this._notifyLoadFailure(result.message, what);
        }
        break;
      }

      items.push(...result.data.items);

      const totalPages = Number(result.data.total_pages) || 1;
      if (page >= totalPages) {
        break;
      }
      if (page === MAX_PAGES) {
        // 还有后续页但已达拉取上限：提示数据可能不完整
        this.showToast(t("viz.data_page_limit_exceeded"), "warning");
      }
    }
    return items;
  }

  async fetchAllDevices() {
    return this._fetchAllPages("/api/resources/devices", "获取设备列表");
  }

  async autoDiscover() {
    try {
      const result = await this.apiPost("/api/resources/topology/auto-discover", {});
      if (result.success) {
        // 键名与展示层读取的 discovered_connections 对齐
        //（此前回退分支的连线数恒显示 0）
        return result.data || { added_nodes: 0, discovered_connections: 0 };
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
