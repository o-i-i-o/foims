const SVG_NS = "http://www.w3.org/2000/svg";

const DEVICE_COLORS = {
  switch: { fill: "#e3f2fd", stroke: "#1976d2" },
  network_device: { fill: "#e3f2fd", stroke: "#1565c0" },
  server: { fill: "#e8f5e9", stroke: "#388e3c" },
  router: { fill: "#fff3e0", stroke: "#f57c00" },
  camera: { fill: "#fce4ec", stroke: "#c62828" },
  phone: { fill: "#f3e5f5", stroke: "#7b1fa2" },
  pc: { fill: "#f5f5f5", stroke: "#9e9e9e" },
  laptop: { fill: "#f5f5f5", stroke: "#757575" },
  printer: { fill: "#fff8e1", stroke: "#f9a825" },
  other: { fill: "#f5f5f5", stroke: "#9e9e9e" },
};

const TYPE_LABELS = {
  switch: "交换机",
  network_device: "网络设备",
  server: "服务器",
  router: "路由器",
  camera: "摄像头",
  phone: "电话",
  pc: "PC",
  laptop: "笔记本",
  printer: "打印机",
  other: "其他",
};

export class TopologyRenderer {
  constructor(core) {
    this.core = core;
    this._connectionPairCount = new Map();
  }

  drawDeviceNode(device) {
    const g = document.createElementNS(SVG_NS, "g");
    g.classList.add("topology-device-node", `type-${device.device_type || "other"}`);
    g.dataset.deviceId = device.device_id;
    g.dataset.tooltip = `${device.device_name || "Unknown"} (${TYPE_LABELS[device.device_type] || device.device_type})`;

    const x = device.x || 100;
    const y = device.y || 100;
    const w = device.width || 200;
    const h = device.height || 100;
    const colors = DEVICE_COLORS[device.device_type] || DEVICE_COLORS.other;

    const rect = document.createElementNS(SVG_NS, "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", w);
    rect.setAttribute("height", h);
    rect.setAttribute("rx", 6);
    rect.setAttribute("ry", 6);
    rect.setAttribute("fill", colors.fill);
    rect.setAttribute("stroke", colors.stroke);
    rect.setAttribute("stroke-width", 1.5);
    g.appendChild(rect);

    const nameText = document.createElementNS(SVG_NS, "text");
    nameText.classList.add("device-name");
    nameText.textContent = device.device_name || "Unknown";
    nameText.setAttribute("x", x + w / 2);
    nameText.setAttribute("y", y + 22);
    nameText.setAttribute("text-anchor", "middle");
    nameText.setAttribute("dominant-baseline", "middle");
    nameText.dataset.relX = 0;
    nameText.dataset.relY = 22 - h / 2 + h / 2;
    nameText.dataset.relY = 22;
    g.appendChild(nameText);

    const typeText = document.createElementNS(SVG_NS, "text");
    typeText.classList.add("device-type-label");
    typeText.textContent = TYPE_LABELS[device.device_type] || device.device_type || "";
    typeText.setAttribute("x", x + w / 2);
    typeText.setAttribute("y", y + 40);
    typeText.setAttribute("text-anchor", "middle");
    typeText.setAttribute("dominant-baseline", "middle");
    typeText.dataset.relX = 0;
    typeText.dataset.relY = 40;
    g.appendChild(typeText);

    const ipText = document.createElementNS(SVG_NS, "text");
    ipText.classList.add("device-ip");
    ipText.textContent = device.ip_address || device.room_name || "";
    ipText.setAttribute("x", x + w / 2);
    ipText.setAttribute("y", y + 58);
    ipText.setAttribute("text-anchor", "middle");
    ipText.setAttribute("dominant-baseline", "middle");
    ipText.dataset.relX = 0;
    ipText.dataset.relY = 58;
    g.appendChild(ipText);

    const directions = ["top", "right", "bottom", "left"];
    directions.forEach((dir) => {
      const anchor = document.createElementNS(SVG_NS, "circle");
      anchor.classList.add("port-anchor");
      anchor.dataset.portDir = dir;
      anchor.setAttribute("r", 5);
      anchor.setAttribute("fill", "#fff");
      anchor.setAttribute("stroke", colors.stroke);
      anchor.setAttribute("stroke-width", 1.5);

      let cx, cy;
      switch (dir) {
        case "top": cx = x + w / 2; cy = y; break;
        case "right": cx = x + w; cy = y + h / 2; break;
        case "bottom": cx = x + w / 2; cy = y + h; break;
        case "left": cx = x; cy = y + h / 2; break;
      }
      anchor.setAttribute("cx", cx);
      anchor.setAttribute("cy", cy);
      anchor.dataset.relCx = cx - x;
      anchor.dataset.relCy = cy - y;
      g.appendChild(anchor);
    });

    this.core.elementsGroup.appendChild(g);
    return g;
  }

  drawConnection(connection) {
    const sourcePos = this.core.getNodePosition(connection.source_device_id);
    const targetPos = this.core.getNodePosition(connection.target_device_id);
    if (!sourcePos || !targetPos) return null;

    const sourceAnchor = this._getBestAnchor(sourcePos, targetPos);
    const targetAnchor = this._getBestAnchor(targetPos, sourcePos);

    const pairKey = this._getPairKey(connection.source_device_id, connection.target_device_id);
    const pairIndex = this._connectionPairCount.get(pairKey) || 0;
    this._connectionPairCount.set(pairKey, pairIndex + 1);
    const parallelOffset = pairIndex * 15;

    const g = document.createElementNS(SVG_NS, "g");
    g.classList.add("topology-connection-group");
    g.dataset.connectionId = connection.id;

    const path = document.createElementNS(SVG_NS, "path");
    path.classList.add("topology-connection");
    if (connection.auto_discovered) {
      path.classList.add("auto-discovered");
    }
    const pathD = this._calculateOrthogonalPath(sourceAnchor, targetAnchor, parallelOffset);
    path.setAttribute("d", pathD);
    g.appendChild(path);

    const sourceLabel = this._createConnectionLabel(
      sourceAnchor.x, sourceAnchor.y,
      connection.source_port_number || connection.source_port_name || "",
      "source"
    );
    if (sourceLabel) g.appendChild(sourceLabel);

    const targetLabel = this._createConnectionLabel(
      targetAnchor.x, targetAnchor.y,
      connection.target_port_number || connection.target_port_name || "",
      "target"
    );
    if (targetLabel) g.appendChild(targetLabel);

    // 渲染信息点链（设备 → 信息点[0] → 信息点[1] → ... → 目标设备）
    const chain = Array.isArray(connection.outlet_chain) ? connection.outlet_chain : [];
    if (chain.length > 0) {
      this._drawOutletChain(g, sourceAnchor, targetAnchor, chain, parallelOffset);
    }

    path.addEventListener("click", (e) => {
      e.stopPropagation();
      if (this.core.callbacks.onConnectionClick) {
        this.core.callbacks.onConnectionClick(connection.id);
      }
    });

    this.core.connectionsGroup.appendChild(g);
    return g;
  }

  /// 在连接线上绘制信息点链的中间节点
  /// 沿着 sourceAnchor → targetAnchor 的直线路径均匀分布
  _drawOutletChain(group, sourceAnchor, targetAnchor, chain, parallelOffset = 0) {
    const n = chain.length;
    // 在 source 和 target 之间均匀分布 n 个点
    for (let i = 0; i < n; i++) {
      const t = (i + 1) / (n + 1);
      const x = sourceAnchor.x + (targetAnchor.x - sourceAnchor.x) * t;
      const y = sourceAnchor.y + (targetAnchor.y - sourceAnchor.y) * t + parallelOffset;

      const node = document.createElementNS(SVG_NS, "g");
      node.classList.add("topology-outlet-node");

      const circle = document.createElementNS(SVG_NS, "circle");
      circle.setAttribute("cx", x);
      circle.setAttribute("cy", y);
      circle.setAttribute("r", 8);
      circle.setAttribute("fill", "#fff3e0");
      circle.setAttribute("stroke", "#f57c00");
      circle.setAttribute("stroke-width", 1.5);
      node.appendChild(circle);

      const label = document.createElementNS(SVG_NS, "text");
      label.classList.add("topology-outlet-label");
      const outletName = chain[i].name || chain[i].id || '';
      label.textContent = outletName;
      label.setAttribute("x", x);
      label.setAttribute("y", y - 14);
      label.setAttribute("text-anchor", "middle");
      label.setAttribute("dominant-baseline", "middle");
      node.appendChild(label);

      const indexLabel = document.createElementNS(SVG_NS, "text");
      indexLabel.classList.add("topology-outlet-index");
      indexLabel.textContent = String(i + 1);
      indexLabel.setAttribute("x", x);
      indexLabel.setAttribute("y", y + 3);
      indexLabel.setAttribute("text-anchor", "middle");
      indexLabel.setAttribute("dominant-baseline", "middle");
      node.appendChild(indexLabel);

      node.dataset.tooltip = `${outletName} (${chain[i].outlet_type || 'outlet'}) - 链路顺序: ${i + 1}/${n}`;
      group.appendChild(node);
    }
  }

  _getPairKey(id1, id2) {
    const sorted = [id1, id2].sort();
    return `${sorted[0]}|${sorted[1]}`;
  }

  _getBestAnchor(sourcePos, targetPos) {
    const cx = sourcePos.x + sourcePos.width / 2;
    const cy = sourcePos.y + sourcePos.height / 2;
    const tcx = targetPos.x + targetPos.width / 2;
    const tcy = targetPos.y + targetPos.height / 2;

    const dx = tcx - cx;
    const dy = tcy - cy;

    if (Math.abs(dx) > Math.abs(dy)) {
      if (dx > 0) return { x: sourcePos.x + sourcePos.width, y: cy, dir: "right" };
      return { x: sourcePos.x, y: cy, dir: "left" };
    }
    if (dy > 0) return { x: cx, y: sourcePos.y + sourcePos.height, dir: "bottom" };
    return { x: cx, y: sourcePos.y, dir: "top" };
  }

  _calculateOrthogonalPath(source, target, parallelOffset = 0) {
    const dx = target.x - source.x;
    const dy = target.y - source.y;

    const midY = source.y + dy * 0.5 + parallelOffset;
    const midX = source.x + dx * 0.5 + parallelOffset;

    if (source.dir === "bottom" || source.dir === "top") {
      const elbowY = source.dir === "bottom" ? midY : source.y - Math.abs(dy) * 0.5 - parallelOffset;
      const clampedEy = source.dir === "bottom"
        ? Math.max(source.y + 20, Math.min(target.y - 20, elbowY))
        : Math.min(source.y - 20, Math.max(target.y + 20, elbowY));
      return `M ${source.x} ${source.y} L ${source.x} ${clampedEy} L ${target.x} ${clampedEy} L ${target.x} ${target.y}`;
    }

    if (source.dir === "right" || source.dir === "left") {
      const elbowX = source.dir === "right" ? midX : source.x - Math.abs(dx) * 0.5 - parallelOffset;
      const clampedEx = source.dir === "right"
        ? Math.max(source.x + 20, Math.min(target.x - 20, elbowX))
        : Math.min(source.x - 20, Math.max(target.x + 20, elbowX));
      return `M ${source.x} ${source.y} L ${clampedEx} ${source.y} L ${clampedEx} ${target.y} L ${target.x} ${target.y}`;
    }

    return `M ${source.x} ${source.y} L ${target.x} ${target.y}`;
  }

  _createConnectionLabel(x, y, text, type) {
    if (!text) return null;
    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("topology-connection-label");
    label.textContent = text;
    label.setAttribute("x", x);
    label.setAttribute("y", type === "source" ? y - 10 : y + 16);
    label.setAttribute("text-anchor", "middle");
    label.setAttribute("dominant-baseline", "middle");
    return label;
  }

  updateConnectionPaths(deviceId) {
    const affectedConnections = this.core.connectionsGroup.querySelectorAll(
      `.topology-connection-group`
    );
    this._connectionPairCount.clear();
    const toRedraw = [];
    affectedConnections.forEach((g) => {
      const connectionId = g.dataset.connectionId;
      const conn = this._connectionsMap?.get(connectionId);
      if (conn) {
        toRedraw.push(conn);
        g.remove();
      }
    });
    toRedraw.forEach((conn) => this.drawConnection(conn));
  }

  setConnectionsMap(map) {
    this._connectionsMap = map;
    this._connectionPairCount.clear();
  }

  clearAll() {
    this.core.elementsGroup.innerHTML = "";
    this.core.connectionsGroup.innerHTML = "";
    this.core.tempConnectionGroup.innerHTML = "";
  }
}
