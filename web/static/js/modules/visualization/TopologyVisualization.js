import { TopologyCore } from "./TopologyCore.js";
import { TopologyRenderer } from "./TopologyRenderer.js";
import { TopologyDataManager } from "./TopologyDataManager.js";
import { showConfirm } from "../../utils/confirm.js";
import { showToast } from "../../utils/ui.js";

export class TopologyVisualization {
  constructor(containerId) {
    this.core = new TopologyCore(containerId, {
      onNodeClick: (deviceId) => this.openDeviceDetail(deviceId),
      onNodeDrag: (deviceId) => this.renderer.updateConnectionPaths(deviceId),
      onNodeDragEnd: () => {},
      onConnectionComplete: (sDev, sPort, tDev, tPort) => this._createConnection(sDev, sPort, tDev, tPort),
      onConnectionClick: (connId) => this._handleConnectionClick(connId),
      onCanvasClick: () => this._deselectConnection(),
    });
    this.renderer = new TopologyRenderer(this.core);
    this.dataManager = new TopologyDataManager();
    this.nodes = [];
    this.connections = [];
    this.connectionsMap = new Map();
    this.selectedConnectionId = null;
  }

  async loadTopology() {
    this.renderer.clearAll();
    const [nodes, connections] = await Promise.all([
      this.dataManager.fetchTopologyNodes(),
      this.dataManager.fetchTopologyConnections(),
    ]);

    this.nodes = nodes;
    this.connections = connections;
    this.connectionsMap.clear();
    connections.forEach((c) => this.connectionsMap.set(c.id, c));
    this.renderer.setConnectionsMap(this.connectionsMap);

    nodes.forEach((node) => this.renderer.drawDeviceNode(node));
    connections.forEach((conn) => this.renderer.drawConnection(conn));

    this._fitView();
    this.core._updateZoomIndicator();
  }

  async addDevice(deviceId, deviceName, deviceType) {
    if (this.nodes.find((n) => n.device_id === deviceId)) {
      showToast("该设备已在拓扑中", "warning");
      return;
    }

    const position = this._findNextPosition();
    const node = {
      device_id: deviceId,
      x: position.x,
      y: position.y,
      width: 200,
      height: 100,
      device_name: deviceName,
      device_type: deviceType,
    };

    const saved = await this.dataManager.saveTopologyNodes([node]);
    if (saved) {
      this.nodes.push(node);
      this.renderer.drawDeviceNode(node);
    }
  }

  async deleteDevice(deviceId) {
    const confirmed = await showConfirm("确定要从拓扑中移除该设备吗？关联连线也会被删除。");
    if (!confirmed) return;

    const success = await this.dataManager.deleteTopologyNode(deviceId);
    if (success) {
      this.nodes = this.nodes.filter((n) => n.device_id !== deviceId);
      this.connections = this.connections.filter(
        (c) => c.source_device_id !== deviceId && c.target_device_id !== deviceId
      );
      this.connectionsMap.clear();
      this.connections.forEach((c) => this.connectionsMap.set(c.id, c));
      await this.loadTopology();
    }
  }

  async _createConnection(sourceDeviceId, sourcePortId, targetDeviceId, targetPortId) {
    const result = await this.dataManager.createConnection({
      source_device_id: sourceDeviceId,
      target_device_id: targetDeviceId,
      source_port_id: sourcePortId || null,
      target_port_id: targetPortId || null,
    });

    if (result) {
      await this.loadTopology();
    }
  }

  async _handleConnectionClick(connectionId) {
    this.selectedConnectionId = connectionId;
    const path = this.core.connectionsGroup.querySelector(`[data-connection-id="${connectionId}"] .topology-connection`);
    if (path) path.classList.add("selected");
  }

  async _deselectConnection() {
    if (this.selectedConnectionId) {
      this.core.connectionsGroup.querySelectorAll(".selected").forEach((el) => el.classList.remove("selected"));
      this.selectedConnectionId = null;
    }
  }

  async deleteSelectedConnection() {
    if (!this.selectedConnectionId) {
      showToast("请先选择一条连线", "warning");
      return;
    }
    const confirmed = await showConfirm("确定要删除此连线吗？");
    if (!confirmed) return;

    const success = await this.dataManager.deleteConnection(this.selectedConnectionId);
    if (success) {
      this.selectedConnectionId = null;
      await this.loadTopology();
    }
  }

  async saveLayout() {
    const elements = this.core.elementsGroup.querySelectorAll("[data-device-id]");
    const nodes = [];
    elements.forEach((el) => {
      const rect = el.querySelector("rect");
      if (!rect) return;
      nodes.push({
        device_id: el.dataset.deviceId,
        x: parseFloat(rect.getAttribute("x")),
        y: parseFloat(rect.getAttribute("y")),
        width: parseFloat(rect.getAttribute("width")),
        height: parseFloat(rect.getAttribute("height")),
      });
    });

    if (nodes.length === 0) {
      showToast("没有可保存的节点", "warning");
      return;
    }

    await this.dataManager.saveTopologyNodes(nodes);
  }

  async deleteLayout() {
    const confirmed = await showConfirm("确定要删除全局拓扑布局吗？此操作不可恢复。");
    if (!confirmed) return;

    for (const node of this.nodes) {
      await this.dataManager.deleteTopologyNode(node.device_id);
    }
    this.nodes = [];
    this.connections = [];
    this.connectionsMap.clear();
    this.renderer.clearAll();
    showToast("拓扑布局删除成功", "success");
  }

  enterConnectionMode() {
    this.core.setConnectionMode(true);
  }

  exitConnectionMode() {
    this.core.setConnectionMode(false);
  }

  toggleConnectionMode() {
    this.core.setConnectionMode(!this.core.isConnectionMode);
    return this.core.isConnectionMode;
  }

  autoLayout() {
    const cols = Math.ceil(Math.sqrt(this.nodes.length));
    const gapX = 250;
    const gapY = 150;
    const startX = 100;
    const startY = 100;

    this.nodes.forEach((node, i) => {
      const col = i % cols;
      const row = Math.floor(i / cols);
      node.x = startX + col * gapX;
      node.y = startY + row * gapY;
    });

    this.renderer.clearAll();
    this.nodes.forEach((node) => this.renderer.drawDeviceNode(node));
    this.connections.forEach((conn) => this.renderer.drawConnection(conn));
    this.saveLayout();
  }

  openDeviceDetail(deviceId) {
    if (this.callbacks?.onDeviceDetail) {
      this.callbacks.onDeviceDetail(deviceId);
    }
  }

  _findNextPosition() {
    const cols = Math.ceil(Math.sqrt(this.nodes.length + 1));
    const idx = this.nodes.length;
    return {
      x: 100 + (idx % cols) * 250,
      y: 100 + Math.floor(idx / cols) * 150,
    };
  }

  _fitView() {
    if (this.nodes.length === 0) {
      this.core.svg.setAttribute("viewBox", "0 0 3000 2000");
      return;
    }
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    this.nodes.forEach((n) => {
      minX = Math.min(minX, n.x);
      minY = Math.min(minY, n.y);
      maxX = Math.max(maxX, n.x + (n.width || 200));
      maxY = Math.max(maxY, n.y + (n.height || 100));
    });
    const padding = 100;
    this.core.svg.setAttribute("viewBox", `${minX - padding} ${minY - padding} ${maxX - minX + padding * 2} ${maxY - minY + padding * 2}`);
  }
}
