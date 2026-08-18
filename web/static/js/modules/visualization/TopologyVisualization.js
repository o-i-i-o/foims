import { TopologyCore } from "./TopologyCore.js";
import { TopologyRenderer } from "./TopologyRenderer.js";
import { TopologyDataManager } from "./TopologyDataManager.js";
import { showConfirm } from "../../utils/confirm.js";
import { showToast } from "../../utils/ui.js";
import { t } from "../../utils/i18n.js";

const SVG_NS = "http://www.w3.org/2000/svg";

export class TopologyVisualization {
  constructor(containerId) {
    this.core = new TopologyCore(containerId, {
      onNodeClick: (deviceId) => this.openDeviceDetail(deviceId),
      onNodeDrag: (deviceId) => this.renderer.updateConnectionPaths(deviceId),
      onNodeDragEnd: () => this._renderContainers(),
      onConnectionComplete: (sDev, sPort, tDev, tPort) =>
        this._createConnection(sDev, sPort, tDev, tPort),
      onConnectionClick: (connId) => this._handleConnectionClick(connId),
      onCanvasClick: () => this._deselectConnection()
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
      this.dataManager.fetchTopologyConnections()
    ]);

    this.nodes = nodes;
    this.connections = connections;
    this.connectionsMap.clear();
    connections.forEach((c) => this.connectionsMap.set(c.id, c));
    this.renderer.setConnectionsMap(this.connectionsMap);

    nodes.forEach((node) => this.renderer.drawDeviceNode(node));
    connections.forEach((conn) => this.renderer.drawConnection(conn));
    this._renderContainers();

    this._fitView();
    this.core._updateZoomIndicator();
  }

  async addDevice(deviceId, deviceName, deviceType) {
    if (this.nodes.find((n) => n.device_id === deviceId)) {
      showToast(t("viz.device_already_in_topology"), "warning");
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
      device_type: deviceType
    };

    const saved = await this.dataManager.saveTopologyNodes([node]);
    if (saved) {
      this.nodes.push(node);
      this.renderer.drawDeviceNode(node);
      this._renderContainers();
    }
  }

  async deleteDevice(deviceId) {
    const confirmed = await showConfirm(t("viz.confirm_remove_device"));
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
      connection_type: "physical",
      source_device_id: sourceDeviceId,
      target_device_id: targetDeviceId,
      source_device_port_id: sourcePortId || null,
      target_device_port_id: targetPortId || null
    });

    if (result) {
      await this.loadTopology();
    }
  }

  async _handleConnectionClick(connectionId) {
    this._deselectConnection();
    this.selectedConnectionId = connectionId;

    const g = this.core.connectionsGroup.querySelector(
      `[data-connection-id="${CSS.escape(connectionId)}"]`
    );
    if (!g) return;

    const path = g.querySelector(".topology-connection");
    if (path) path.classList.add("selected");

    const conn = this.connectionsMap.get(connectionId);
    if (!conn) return;

    // 派生物理连线来源于线路数据，需在线路模块删除
    if (conn.derived) {
      showToast(t("viz.physical_derived_hint"), "info");
      return;
    }

    g.classList.add("selected-group");
    const bbox = path.getBBox();
    const midPoint = { x: bbox.x + bbox.width / 2, y: bbox.y + bbox.height / 2 };
    const marker = this.renderer.drawDeleteMarker(g, connectionId, midPoint);
    marker.addEventListener("click", async (e) => {
      e.stopPropagation();
      await this._deleteConnection(connectionId);
    });
  }

  async _deleteConnection(connectionId) {
    const confirmed = await showConfirm(t("viz.connection_delete_confirm"));
    if (!confirmed) return;

    const success = await this.dataManager.deleteConnection(connectionId);
    if (success) {
      this.selectedConnectionId = null;
      await this.loadTopology();
    }
  }

  _deselectConnection() {
    if (!this.selectedConnectionId) return;
    this.core.connectionsGroup
      .querySelectorAll(".selected, .selected-group, .conn-delete-marker")
      .forEach((el) => {
        if (el.classList.contains("conn-delete-marker")) {
          el.remove();
        } else {
          el.classList.remove("selected", "selected-group");
        }
      });
    this.selectedConnectionId = null;
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
        height: parseFloat(rect.getAttribute("height"))
      });
    });

    if (nodes.length === 0) {
      showToast(t("viz.no_nodes_to_save"), "warning");
      return;
    }

    await this.dataManager.saveTopologyNodes(nodes);
  }

  async deleteLayout() {
    const confirmed = await showConfirm(t("viz.confirm_delete_topology_layout"));
    if (!confirmed) return;

    for (const node of this.nodes) {
      await this.dataManager.deleteTopologyNode(node.device_id);
    }
    this.nodes = [];
    this.connections = [];
    this.connectionsMap.clear();
    this.renderer.clearAll();
    showToast(t("viz.topology_layout_delete_success"), "success");
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
        `${t("viz.auto_discover_done", { count: result.added_nodes })}，${result.discovered_connections ?? 0} ${t("viz.connections_unit")}`,
        "success"
      );
    }
  }

  autoLayout() {
    this.hierarchicalLayout();
  }

  /// 区域容器：同一组织/房间/机柜的设备放入同一容器框
  _renderContainers() {
    const container = this.core.containersGroup;
    container.innerHTML = "";

    const groups = new Map();
    this.core.elementsGroup.querySelectorAll("[data-device-id]").forEach((el) => {
      const rect = el.querySelector("rect");
      if (!rect) return;
      const node = this.nodes.find((n) => n.device_id === el.dataset.deviceId);
      if (!node) return;

      const org = node.org_name || t("viz.group_no_org");
      const room = node.room_name || t("viz.group_no_room");
      const cabinet = node.cabinet_name || t("viz.group_no_cabinet");
      const key = `${org} / ${room} / ${cabinet}`;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push({
        x: parseFloat(rect.getAttribute("x")),
        y: parseFloat(rect.getAttribute("y")),
        width: parseFloat(rect.getAttribute("width")) || 200,
        height: parseFloat(rect.getAttribute("height")) || 100
      });
    });

    const PADDING = 40;
    const TOP = 52;

    groups.forEach((members, key) => {
      let minX = Infinity,
        minY = Infinity,
        maxX = -Infinity,
        maxY = -Infinity;
      members.forEach((m) => {
        minX = Math.min(minX, m.x);
        minY = Math.min(minY, m.y);
        maxX = Math.max(maxX, m.x + m.width);
        maxY = Math.max(maxY, m.y + m.height);
      });

      const g = document.createElementNS(SVG_NS, "g");
      g.classList.add("topology-container-group");

      const rect = document.createElementNS(SVG_NS, "rect");
      rect.classList.add("topology-container");
      rect.setAttribute("x", minX - PADDING);
      rect.setAttribute("y", minY - PADDING - TOP);
      rect.setAttribute("width", maxX - minX + PADDING * 2);
      rect.setAttribute("height", maxY - minY + PADDING * 2 + TOP);
      rect.setAttribute("rx", 10);
      g.appendChild(rect);

      const label = document.createElementNS(SVG_NS, "text");
      label.classList.add("topology-container-label");
      label.textContent = key;
      label.setAttribute("x", minX - PADDING + 14);
      label.setAttribute("y", minY - PADDING - TOP + 20);
      g.appendChild(label);

      const count = document.createElementNS(SVG_NS, "text");
      count.classList.add("topology-container-count");
      count.textContent = `${members.length} ${t("viz.device_unit")}`;
      count.setAttribute("x", minX - PADDING + 14);
      count.setAttribute("y", minY - PADDING - TOP + 38);
      g.appendChild(count);

      container.appendChild(g);
    });
  }

  hierarchicalLayout() {
    if (this.nodes.length === 0) {
      showToast(t("viz.no_nodes_to_layout"), "warning");
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
    this._renderContainers();
    this._fitView();
    this.saveLayout();
  }

  _getFirstPortNumber(deviceId) {
    const conns = this.connections.filter(
      (c) => c.source_device_id === deviceId || c.target_device_id === deviceId
    );
    for (const c of conns) {
      const num =
        c.source_device_id === deviceId ? c.source_port_label : c.target_port_label;
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
      y: 100 + Math.floor(idx / cols) * 150
    };
  }

  _fitView() {
    if (this.nodes.length === 0) {
      this.core.svg.setAttribute("viewBox", "0 0 3000 2000");
      return;
    }
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
    // 视野包含区域容器（组织/房间/机柜分组框）
    const containerRects = this.core.containersGroup.querySelectorAll(".topology-container");
    if (containerRects.length > 0) {
      containerRects.forEach((rect) => {
        minX = Math.min(minX, parseFloat(rect.getAttribute("x")));
        minY = Math.min(minY, parseFloat(rect.getAttribute("y")));
        maxX = Math.max(maxX, parseFloat(rect.getAttribute("x")) + parseFloat(rect.getAttribute("width")));
        maxY = Math.max(maxY, parseFloat(rect.getAttribute("y")) + parseFloat(rect.getAttribute("height")));
      });
    } else {
      this.nodes.forEach((n) => {
        minX = Math.min(minX, n.x);
        minY = Math.min(minY, n.y);
        maxX = Math.max(maxX, n.x + (n.width || 200));
        maxY = Math.max(maxY, n.y + (n.height || 100));
      });
    }
    const padding = 60;
    this.core.svg.setAttribute(
      "viewBox",
      `${minX - padding} ${minY - padding} ${maxX - minX + padding * 2} ${maxY - minY + padding * 2}`
    );
  }
}
