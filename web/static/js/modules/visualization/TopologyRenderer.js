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

    const g = document.createElementNS(SVG_NS, "g");
    g.classList.add("topology-connection-group");
    g.dataset.connectionId = connection.id;

    const path = document.createElementNS(SVG_NS, "path");
    path.classList.add("topology-connection");
    const pathD = this._calculateCurvePath(sourceAnchor, targetAnchor);
    path.setAttribute("d", pathD);
    g.appendChild(path);

    const midX = (sourceAnchor.x + targetAnchor.x) / 2;
    const midY = (sourceAnchor.y + targetAnchor.y) / 2;

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

    path.addEventListener("click", (e) => {
      e.stopPropagation();
      if (this.core.callbacks.onConnectionClick) {
        this.core.callbacks.onConnectionClick(connection.id);
      }
    });

    this.core.connectionsGroup.appendChild(g);
    return g;
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

  _calculateCurvePath(source, target) {
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const dist = Math.sqrt(dx * dx + dy * dy);
    const offset = Math.min(dist * 0.3, 80);

    const sControl = this._getControlPoint(source, offset);
    const tControl = this._getControlPoint(target, offset);

    return `M ${source.x} ${source.y} C ${sControl.x} ${sControl.y}, ${tControl.x} ${tControl.y}, ${target.x} ${target.y}`;
  }

  _getControlPoint(point, offset) {
    switch (point.dir) {
      case "top": return { x: point.x, y: point.y - offset };
      case "right": return { x: point.x + offset, y: point.y };
      case "bottom": return { x: point.x, y: point.y + offset };
      case "left": return { x: point.x - offset, y: point.y };
      default: return { x: point.x, y: point.y - offset };
    }
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
    affectedConnections.forEach((g) => {
      const connectionId = g.dataset.connectionId;
      const conn = this._connectionsMap?.get(connectionId);
      if (conn && (conn.source_device_id === deviceId || conn.target_device_id === deviceId)) {
        g.remove();
        this.drawConnection(conn);
      }
    });
  }

  setConnectionsMap(map) {
    this._connectionsMap = map;
  }

  clearAll() {
    this.core.elementsGroup.innerHTML = "";
    this.core.connectionsGroup.innerHTML = "";
    this.core.tempConnectionGroup.innerHTML = "";
  }
}
