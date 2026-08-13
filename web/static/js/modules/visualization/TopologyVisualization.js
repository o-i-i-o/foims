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

  toggleConnectionMode() {
    this.core.setConnectionMode(!this.core.isConnectionMode);
    return this.core.isConnectionMode;
  }

  async autoDiscover() {
    const result = await this.dataManager.autoDiscover();
    if (result) {
      await this.loadTopology();
      this.hierarchicalLayout();
      showToast(
        `自动发现完成：新增 ${result.added_nodes} 个节点，${result.added_connections} 条连线`,
        "success"
      );
    }
  }

  autoLayout() {
    this.hierarchicalLayout();
  }

  hierarchicalLayout() {
    if (this.nodes.length === 0) {
      showToast("没有可布局的节点", "warning");
      return;
    }

    const nodeMap = new Map();
    this.nodes.forEach((n) => nodeMap.set(n.device_id, n));

    const adjacency = new Map();
    this.nodes.forEach((n) => adjacency.set(n.device_id, []));
    this.connections.forEach((conn) => {
      if (adjacency.has(conn.source_device_id) && adjacency.has(conn.target_device_id)) {
        adjacency.get(conn.source_device_id).push(conn.target_device_id);
        adjacency.get(conn.target_device_id).push(conn.source_device_id);
      }
    });

    const SWITCH_TYPES = ["switch", "router", "network_device"];
    const roots = this.nodes
      .filter((n) => SWITCH_TYPES.includes(n.device_type))
      .map((n) => n.device_id);

    if (roots.length === 0) {
      const sorted = [...this.nodes].sort((a, b) => {
        const aDeg = adjacency.get(a.device_id)?.length || 0;
        const bDeg = adjacency.get(b.device_id)?.length || 0;
        return bDeg - aDeg;
      });
      if (sorted.length > 0) roots.push(sorted[0].device_id);
    }

    const levels = new Map();
    const queue = roots.map((id) => ({ id, level: 0 }));
    const visited = new Set();

    while (queue.length > 0) {
      const { id, level } = queue.shift();
      if (visited.has(id)) {
        if (level < (levels.get(id) ?? Infinity)) {
          levels.set(id, level);
        } else {
          continue;
        }
      }
      visited.add(id);
      levels.set(id, level);

      const neighbors = adjacency.get(id) || [];
      neighbors.forEach((neighborId) => {
        if (!visited.has(neighborId)) {
          queue.push({ id: neighborId, level: level + 1 });
        }
      });
    }

    this.nodes.forEach((n) => {
      if (!levels.has(n.device_id)) levels.set(n.device_id, 0);
    });

    const levelGroups = new Map();
    levels.forEach((level, id) => {
      if (!levelGroups.has(level)) levelGroups.set(level, []);
      levelGroups.get(level).push(id);
    });

    const NODE_WIDTH = 200;
    const NODE_HEIGHT = 100;
    const GAP_X = 60;
    const GAP_Y = 100;
    const START_X = 100;
    const START_Y = 100;

    const sortedLevels = [...levelGroups.keys()].sort((a, b) => a - b);
    const maxLevelWidth = Math.max(
      ...sortedLevels.map((lvl) => {
        const group = levelGroups.get(lvl);
        return group.length * (NODE_WIDTH + GAP_X);
      })
    );

    sortedLevels.forEach((level) => {
      const group = levelGroups.get(level);
      group.sort((a, b) => {
        const portA = this._getFirstPortNumber(a);
        const portB = this._getFirstPortNumber(b);
        if (portA !== null && portB !== null) return portA - portB;
        return 0;
      });

      const totalWidth = group.length * (NODE_WIDTH + GAP_X) - GAP_X;
      const startX = START_X + (maxLevelWidth - totalWidth) / 2;
      const y = START_Y + level * (NODE_HEIGHT + GAP_Y);

      group.forEach((id, i) => {
        const node = nodeMap.get(id);
        if (node) {
          node.x = Math.round(startX + i * (NODE_WIDTH + GAP_X));
          node.y = Math.round(y);
          node.width = NODE_WIDTH;
          node.height = NODE_HEIGHT;
        }
      });
    });

    this._resolveOverlaps(nodeMap, NODE_WIDTH, NODE_HEIGHT, GAP_X, GAP_Y);

    this.renderer.clearAll();
    this.nodes.forEach((node) => this.renderer.drawDeviceNode(node));
    this.connections.forEach((conn) => this.renderer.drawConnection(conn));
    this._fitView();
    this.saveLayout();
  }

  _getFirstPortNumber(deviceId) {
    const conns = this.connections.filter(
      (c) => c.source_device_id === deviceId || c.target_device_id === deviceId
    );
    for (const c of conns) {
      const num = c.source_device_id === deviceId ? c.source_port_number : c.target_port_number;
      if (num) {
        const parsed = parseInt(num, 10);
        if (!isNaN(parsed)) return parsed;
      }
    }
    return null;
  }

  _resolveOverlaps(nodeMap, nodeWidth, nodeHeight, gapX, gapY) {
    const nodes = [...nodeMap.values()];
    const minDistX = nodeWidth + gapX;
    const minDistY = nodeHeight + gapY;

    for (let iter = 0; iter < 20; iter++) {
      let moved = false;
      for (let i = 0; i < nodes.length; i++) {
        for (let j = i + 1; j < nodes.length; j++) {
          const a = nodes[i];
          const b = nodes[j];
          const dx = b.x - a.x;
          const dy = b.y - a.y;
          const overlapX = Math.abs(dx) < minDistX;
          const overlapY = Math.abs(dy) < minDistY;
          if (overlapX && overlapY) {
            const pushX = (minDistX - Math.abs(dx)) / 2 + 5;
            const pushY = (minDistY - Math.abs(dy)) / 2 + 5;
            if (Math.abs(dx) <= Math.abs(dy)) {
              const sign = dx >= 0 ? 1 : -1;
              a.x -= Math.round(sign * pushX);
              b.x += Math.round(sign * pushX);
            } else {
              const sign = dy >= 0 ? 1 : -1;
              a.y -= Math.round(sign * pushY);
              b.y += Math.round(sign * pushY);
            }
            moved = true;
          }
        }
      }
      if (!moved) break;
    }
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
