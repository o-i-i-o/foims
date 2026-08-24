import { t } from "../../utils/i18n.js";

const SVG_NS = "http://www.w3.org/2000/svg";

const DEVICE_COLORS = {
  switch: { fill: "#e3f2fd", stroke: "#1976d2" },
  network_device: { fill: "#e3f2fd", stroke: "#1565c0" },
  server: { fill: "#e8f5e9", stroke: "#388e3c" },
  router: { fill: "#fff3e0", stroke: "#f57c00" },
  camera: { fill: "#fce4ec", stroke: "#c62828" },
  phone: { fill: "#f3e5f5", stroke: "#7b1fa2" },
  desktop: { fill: "#f5f5f5", stroke: "#9e9e9e" },
  laptop: { fill: "#f5f5f5", stroke: "#757575" },
  printer: { fill: "#fff8e1", stroke: "#f9a825" },
  other: { fill: "#f5f5f5", stroke: "#9e9e9e" }
};

const DEVICE_TYPE_I18N_KEYS = {
  switch: "device_type.switch",
  network_device: "device_type.network_device",
  server: "device_type.server",
  router: "device_type.router",
  camera: "device_type.camera",
  phone: "device_type.phone",
  desktop: "device_type.desktop",
  laptop: "device_type.laptop",
  printer: "device_type.printer",
  other: "device_type.other"
};

// 中间节点样式：信息点（圆形/橙）与配线架（方形/蓝紫）
const HOP_STYLES = {
  net_outlet: { kind: "circle", fill: "#fff3e0", stroke: "#f57c00" },
  patch_panel: { kind: "rect", fill: "#ede7f6", stroke: "#5e35b1" }
};

// 同侧边缘上相邻锚点的错开间距
const ANCHOR_SPREAD = 18;

function getDeviceTypeLabel(type) {
  const key = DEVICE_TYPE_I18N_KEYS[type];
  return key ? t(key) : type;
}

export class TopologyRenderer {
  constructor(core) {
    this.core = core;
    this._connectionPairCount = new Map();
    // 设备同侧边缘的锚点占用计数：多条连线共用同一锚点会导致端口标签重叠
    this._anchorSlots = new Map();
  }

  drawDeviceNode(device) {
    const g = document.createElementNS(SVG_NS, "g");
    g.classList.add("topology-device-node", `type-${device.device_type || "other"}`);
    g.dataset.deviceId = device.device_id;
    g.dataset.tooltip = `${device.device_name || "Unknown"} (${getDeviceTypeLabel(device.device_type) || device.device_type})`;
    // 分组键与 TopologyVisualization._collectSpatialGroups 一致，
    // 容器（房间/机柜分组框）拖动时按此键匹配并整体移动组内节点
    g.dataset.roomKey = device.room_id ? `room:${device.room_id}` : "room:none";
    const cabinetKey = device.cabinet_id
      ? `cab:${device.cabinet_id}`
      : device.cabinet_name
        ? `cab:name:${device.cabinet_name}`
        : null;
    if (cabinetKey) {
      g.dataset.cabinetKey = cabinetKey;
    }

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
    nameText.dataset.relY = 22;
    g.appendChild(nameText);

    const typeText = document.createElementNS(SVG_NS, "text");
    typeText.classList.add("device-type-label");
    typeText.textContent = getDeviceTypeLabel(device.device_type) || device.device_type || "";
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
        case "top":
          cx = x + w / 2;
          cy = y;
          break;
        case "right":
          cx = x + w;
          cy = y + h / 2;
          break;
        case "bottom":
          cx = x + w / 2;
          cy = y + h;
          break;
        case "left":
          cx = x;
          cy = y + h / 2;
          break;
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

  /// 绘制连线：按类型分派
  /// - 派生物理连线（derived，含 hops/cables）：折线穿过中间节点，逐段标注线路
  /// - 手动物理示意（manual）：正交折线 + 虚线
  /// - 逻辑连接（logical）：加粗紫色虚线 + LAG 徽标 + 成员端口
  drawConnection(connection) {
    const sourcePos = this.core.getNodePosition(connection.source_device_id);
    const targetPos = this.core.getNodePosition(connection.target_device_id);
    if (!sourcePos || !targetPos) return null;

    const sourceAnchor = this._getBestAnchor(connection.source_device_id, sourcePos, targetPos);
    const targetAnchor = this._getBestAnchor(connection.target_device_id, targetPos, sourcePos);

    const pairKey = this._getPairKey(connection.source_device_id, connection.target_device_id);
    const pairIndex = this._connectionPairCount.get(pairKey) || 0;
    this._connectionPairCount.set(pairKey, pairIndex + 1);
    const parallelOffset = pairIndex * 15;

    const g = document.createElementNS(SVG_NS, "g");
    g.classList.add("topology-connection-group");
    g.dataset.connectionId = connection.id;
    g.dataset.connectionType = connection.connection_type;
    g.dataset.derived = connection.derived ? "true" : "false";

    const path = document.createElementNS(SVG_NS, "path");
    path.classList.add("topology-connection");
    if (connection.connection_type === "logical") {
      path.classList.add("logical");
      this._drawLogicalConnection(g, path, connection, sourceAnchor, targetAnchor, parallelOffset);
    } else {
      if (connection.derived) {
        path.classList.add("auto-discovered");
      } else {
        path.classList.add("manual");
      }
      this._drawPhysicalConnection(g, path, connection, sourceAnchor, targetAnchor, parallelOffset);
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

  /// 物理连线：途经信息点/配线架时按多段折线绘制
  _drawPhysicalConnection(group, path, connection, sourceAnchor, targetAnchor, parallelOffset) {
    const hops = Array.isArray(connection.hops) ? connection.hops : [];
    const cables = Array.isArray(connection.cables) ? connection.cables : [];

    if (hops.length === 0) {
      path.setAttribute(
        "d",
        this._calculateOrthogonalPath(sourceAnchor, targetAnchor, parallelOffset)
      );
      group.appendChild(path);
    } else {
      // 中间节点在两锚点之间均匀分布，路径逐段折线连接
      const points = [sourceAnchor];
      const hopPoints = hops.map((hop, i) => {
        const ratio = (i + 1) / (hops.length + 1);
        return {
          x: sourceAnchor.x + (targetAnchor.x - sourceAnchor.x) * ratio,
          y: sourceAnchor.y + (targetAnchor.y - sourceAnchor.y) * ratio + parallelOffset
        };
      });
      points.push(...hopPoints, targetAnchor);

      path.setAttribute(
        "d",
        points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ")
      );
      group.appendChild(path);

      hops.forEach((hop, i) => {
        this._drawHopNode(group, hop, hopPoints[i], i, hops.length);
        this._drawCableLabel(group, cables[i], points[i], points[i + 1]);
      });
      this._drawCableLabel(
        group,
        cables[hops.length],
        points[hops.length],
        points[hops.length + 1]
      );
    }

    const sourceLabel = this._createConnectionLabel(
      sourceAnchor.x,
      sourceAnchor.y,
      connection.source_port_label || "",
      "source"
    );
    if (sourceLabel) group.appendChild(sourceLabel);

    const targetLabel = this._createConnectionLabel(
      targetAnchor.x,
      targetAnchor.y,
      connection.target_port_label || "",
      "target"
    );
    if (targetLabel) group.appendChild(targetLabel);
  }

  /// 逻辑连接（链路聚合）：加粗虚线 + 中点徽标 + 成员端口摘要
  _drawLogicalConnection(group, path, connection, sourceAnchor, targetAnchor, parallelOffset) {
    path.setAttribute(
      "d",
      this._calculateOrthogonalPath(sourceAnchor, targetAnchor, parallelOffset)
    );
    group.appendChild(path);

    const midX = (sourceAnchor.x + targetAnchor.x) / 2;
    const midY = (sourceAnchor.y + targetAnchor.y) / 2 + parallelOffset;

    const badge = document.createElementNS(SVG_NS, "g");
    badge.classList.add("topology-lag-badge");

    const badgeText = connection.label || t("viz.lag_badge");
    const badgeRect = document.createElementNS(SVG_NS, "rect");
    const textWidth = Math.max(badgeText.length * 8 + 20, 64);
    badgeRect.setAttribute("x", midX - textWidth / 2);
    badgeRect.setAttribute("y", midY - 12);
    badgeRect.setAttribute("width", textWidth);
    badgeRect.setAttribute("height", 24);
    badgeRect.setAttribute("rx", 12);
    badge.appendChild(badgeRect);

    const badgeLabel = document.createElementNS(SVG_NS, "text");
    badgeLabel.textContent = badgeText;
    badgeLabel.setAttribute("x", midX);
    badgeLabel.setAttribute("y", midY + 4);
    badgeLabel.setAttribute("text-anchor", "middle");
    badgeLabel.setAttribute("dominant-baseline", "middle");
    badge.appendChild(badgeLabel);
    group.appendChild(badge);

    const summary = [
      connection.source_members?.length || 0,
      connection.target_members?.length || 0
    ].join(" + ");
    const countLabel = document.createElementNS(SVG_NS, "text");
    countLabel.classList.add("topology-lag-members");
    countLabel.textContent = `${t("viz.member_ports")}: ${summary}`;
    countLabel.setAttribute("x", midX);
    countLabel.setAttribute("y", midY + 26);
    countLabel.setAttribute("text-anchor", "middle");
    group.appendChild(countLabel);

    const memberLabel = (anchor, members, type) => {
      if (!members || members.length === 0) return null;
      const text = members
        .slice(0, 4)
        .map((m) => m.port_number || m.port_id.slice(0, 8))
        .join(", ");
      return this._createConnectionLabel(
        anchor.x,
        anchor.y,
        members.length > 4 ? `${text}…` : text,
        type
      );
    };
    const srcLabel = memberLabel(sourceAnchor, connection.source_members, "source");
    if (srcLabel) group.appendChild(srcLabel);
    const tgtLabel = memberLabel(targetAnchor, connection.target_members, "target");
    if (tgtLabel) group.appendChild(tgtLabel);
  }

  /// 绘制途经的中间节点（信息点=圆形，配线架=方形）
  _drawHopNode(group, hop, point, index, total) {
    const style = HOP_STYLES[hop.node_type] || HOP_STYLES.net_outlet;
    const node = document.createElementNS(SVG_NS, "g");
    node.classList.add(
      "topology-hop-node",
      hop.node_type === "patch_panel" ? "hop-patch-panel" : "hop-net-outlet"
    );

    if (style.kind === "rect") {
      const rect = document.createElementNS(SVG_NS, "rect");
      rect.setAttribute("x", point.x - 9);
      rect.setAttribute("y", point.y - 9);
      rect.setAttribute("width", 18);
      rect.setAttribute("height", 18);
      rect.setAttribute("rx", 3);
      rect.setAttribute("fill", style.fill);
      rect.setAttribute("stroke", style.stroke);
      rect.setAttribute("stroke-width", 1.5);
      node.appendChild(rect);
    } else {
      const circle = document.createElementNS(SVG_NS, "circle");
      circle.setAttribute("cx", point.x);
      circle.setAttribute("cy", point.y);
      circle.setAttribute("r", 9);
      circle.setAttribute("fill", style.fill);
      circle.setAttribute("stroke", style.stroke);
      circle.setAttribute("stroke-width", 1.5);
      node.appendChild(circle);
    }

    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("topology-outlet-label");
    const hopName = hop.node_label || hop.node_id || "";
    label.textContent = hopName;
    label.setAttribute("x", point.x);
    label.setAttribute("y", point.y - 15);
    label.setAttribute("text-anchor", "middle");
    label.setAttribute("dominant-baseline", "middle");
    node.appendChild(label);

    const indexLabel = document.createElementNS(SVG_NS, "text");
    indexLabel.classList.add("topology-outlet-index");
    indexLabel.textContent = String(index + 1);
    indexLabel.setAttribute("x", point.x);
    indexLabel.setAttribute("y", point.y + 3.5);
    indexLabel.setAttribute("text-anchor", "middle");
    indexLabel.setAttribute("dominant-baseline", "middle");
    node.appendChild(indexLabel);

    const typeLabel =
      hop.node_type === "patch_panel" ? t("viz.patch_panel_node") : t("viz.net_outlet_node");
    node.dataset.tooltip = `${typeLabel} ${hopName} - ${t("viz.link_order")}: ${index + 1}/${total}`;
    group.appendChild(node);
  }

  /// 在一段线路中点标注线缆标签
  _drawCableLabel(group, cable, from, to) {
    if (!cable || !cable.cable_label) return;
    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("topology-cable-label");
    label.textContent = cable.cable_label;
    label.setAttribute("x", (from.x + to.x) / 2);
    label.setAttribute("y", (from.y + to.y) / 2 - 6);
    label.setAttribute("text-anchor", "middle");
    label.setAttribute("dominant-baseline", "middle");
    group.appendChild(label);
  }

  /// 在连线上叠加删除标记（仅存储连线可删，派生物理连线由线路管理）
  drawDeleteMarker(group, connectionId, midPoint) {
    const marker = document.createElementNS(SVG_NS, "g");
    marker.classList.add("conn-delete-marker");
    marker.dataset.connectionId = connectionId;

    const circle = document.createElementNS(SVG_NS, "circle");
    circle.setAttribute("cx", midPoint.x);
    circle.setAttribute("cy", midPoint.y - 26);
    circle.setAttribute("r", 9);
    marker.appendChild(circle);

    const cross = document.createElementNS(SVG_NS, "text");
    cross.textContent = "×";
    cross.setAttribute("x", midPoint.x);
    cross.setAttribute("y", midPoint.y - 22.5);
    cross.setAttribute("text-anchor", "middle");
    cross.setAttribute("dominant-baseline", "middle");
    marker.appendChild(cross);

    marker.dataset.tooltip = t("viz.delete_connection");
    group.appendChild(marker);
    return marker;
  }

  _getPairKey(id1, id2) {
    const sorted = [id1, id2].sort();
    return `${sorted[0]}|${sorted[1]}`;
  }

  _getBestAnchor(deviceId, sourcePos, targetPos) {
    const cx = sourcePos.x + sourcePos.width / 2;
    const cy = sourcePos.y + sourcePos.height / 2;
    const tcx = targetPos.x + targetPos.width / 2;
    const tcy = targetPos.y + targetPos.height / 2;

    const dx = tcx - cx;
    const dy = tcy - cy;

    let anchor;
    if (Math.abs(dx) > Math.abs(dy)) {
      if (dx > 0) anchor = { x: sourcePos.x + sourcePos.width, y: cy, dir: "right" };
      else anchor = { x: sourcePos.x, y: cy, dir: "left" };
    } else if (dy > 0) {
      anchor = { x: cx, y: sourcePos.y + sourcePos.height, dir: "bottom" };
    } else {
      anchor = { x: cx, y: sourcePos.y, dir: "top" };
    }

    // 同侧边缘按连接次序左右/上下错开，避免多条连线共用同一锚点
    // 导致端口标签相互覆盖（次序：中心、+S、-S、+2S、-2S…）
    const slot = this._anchorSlots.get(`${deviceId}:${anchor.dir}`) || 0;
    this._anchorSlots.set(`${deviceId}:${anchor.dir}`, slot + 1);
    const magnitude = Math.floor((slot + 1) / 2);
    const offset = magnitude * (slot % 2 === 0 ? -ANCHOR_SPREAD : ANCHOR_SPREAD);

    if (anchor.dir === "top" || anchor.dir === "bottom") {
      anchor.x = Math.max(
        sourcePos.x + 16,
        Math.min(sourcePos.x + sourcePos.width - 16, anchor.x + offset)
      );
    } else {
      anchor.y = Math.max(
        sourcePos.y + 16,
        Math.min(sourcePos.y + sourcePos.height - 16, anchor.y + offset)
      );
    }
    return anchor;
  }

  _calculateOrthogonalPath(source, target, parallelOffset = 0) {
    const dx = target.x - source.x;
    const dy = target.y - source.y;

    const midY = source.y + dy * 0.5 + parallelOffset;
    const midX = source.x + dx * 0.5 + parallelOffset;

    if (source.dir === "bottom" || source.dir === "top") {
      const elbowY =
        source.dir === "bottom" ? midY : source.y - Math.abs(dy) * 0.5 - parallelOffset;
      const clampedEy =
        source.dir === "bottom"
          ? Math.max(source.y + 20, Math.min(target.y - 20, elbowY))
          : Math.min(source.y - 20, Math.max(target.y + 20, elbowY));
      return `M ${source.x} ${source.y} L ${source.x} ${clampedEy} L ${target.x} ${clampedEy} L ${target.x} ${target.y}`;
    }

    if (source.dir === "right" || source.dir === "left") {
      const elbowX = source.dir === "right" ? midX : source.x - Math.abs(dx) * 0.5 - parallelOffset;
      const clampedEx =
        source.dir === "right"
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
    this._anchorSlots.clear();
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
    this._anchorSlots.clear();
  }

  clearAll() {
    this.core.containersGroup.innerHTML = "";
    this.core.elementsGroup.innerHTML = "";
    this.core.connectionsGroup.innerHTML = "";
    this.core.tempConnectionGroup.innerHTML = "";
    // 计数器随画布清空一并重置，否则重载后平行偏移/锚点错开量会持续累加
    this._connectionPairCount.clear();
    this._anchorSlots.clear();
  }
}
