import { t } from "../../utils/i18n.js";
import { SVG_NS } from "./SVGCore.js";

// 文本锚点/基线居中（设备名、类型、IP 等标签的通用设置，成对出现）
function centerText(textEl) {
  textEl.setAttribute("text-anchor", "middle");
  textEl.setAttribute("dominant-baseline", "middle");
}

/**
 * 机柜分组键：优先按 id，无 id 时按名称；不属于任何机柜返回 null。
 * 渲染与容器拖动两处共用，分组语义必须一致。
 */
export function cabinetGroupKey(node) {
  if (node.cabinet_id) {
    return `cab:${node.cabinet_id}`;
  }
  if (node.cabinet_name) {
    return `cab:name:${node.cabinet_name}`;
  }
  return null;
}

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

// 按设备类型维护的彩色图标（24x24 viewBox，扁平填充风格，
// 每种类型 2~4 种语义色，随节点一起拖动）：
// 交换机=机身+端口+状态灯 / 网络设备=芯片 / 服务器=盘位+指示灯 /
// 路由器=地球 / 摄像头=机身+镜头+录制灯 / 电话=话机+屏幕+键盘 /
// 台式机=显示器+底座 / 笔记本=屏幕+键盘座 / 打印机=机身+纸张+CMY /
// 其他=包装箱+胶带
const DEVICE_ICONS = {
  // 交换机：深灰机箱 + 浅灰端口排 + 绿/黄状态灯
  switch: `
    <rect x="1.5" y="8" width="21" height="8.5" rx="2" fill="#37474f"/>
    <rect x="1.5" y="8" width="21" height="3" rx="1.5" fill="#546e7a"/>
    <rect x="4.2" y="12" width="8.6" height="3" rx="0.6" fill="#b0bec5"/>
    <path d="M5.4 13.5h6.2" stroke="#78909c" stroke-width="0.8"/>
    <path d="M5.4 12h6.2M8.5 12v3" stroke="#90a4ae" stroke-width="0.5" fill="none"/>
    <circle cx="16.2" cy="13.5" r="0.85" fill="#66bb6a"/>
    <circle cx="18.4" cy="13.5" r="0.85" fill="#66bb6a"/>
    <circle cx="20.6" cy="13.5" r="0.85" fill="#ffca28"/>`,
  // 网络设备：深灰芯片 + 蓝色内核 + 金色引脚
  network_device: `
    <path d="M9 1v3M15 1v3M9 20v3M15 20v3M1 9h3M1 15h3M20 9h3M20 15h3" stroke="#ffb300" stroke-width="1.6" stroke-linecap="round"/>
    <rect x="5.5" y="5.5" width="13" height="13" rx="2" fill="#455a64"/>
    <rect x="9" y="9" width="6" height="6" rx="1" fill="#42a5f5"/>
    <path d="M12 9v6M9 12h6" stroke="#90caf9" stroke-width="0.8"/>`,
  // 服务器：机架 + 三层盘位 + 绿色指示灯
  server: `
    <rect x="5" y="2" width="14" height="20" rx="2" fill="#37474f"/>
    <rect x="7" y="4" width="10" height="4" rx="0.8" fill="#78909c"/>
    <rect x="7" y="10" width="10" height="4" rx="0.8" fill="#78909c"/>
    <rect x="7" y="16" width="10" height="4" rx="0.8" fill="#78909c"/>
    <path d="M8.2 6h4.6M8.2 12h4.6M8.2 18h4.6" stroke="#42a5f5" stroke-width="0.9" stroke-linecap="round"/>
    <circle cx="15.4" cy="6" r="0.8" fill="#66bb6a"/>
    <circle cx="15.4" cy="12" r="0.8" fill="#66bb6a"/>
    <circle cx="15.4" cy="18" r="0.8" fill="#66bb6a"/>`,
  // 路由器：蓝色地球 + 浅色经纬网
  router: `
    <circle cx="12" cy="12" r="9.5" fill="#1e88e5"/>
    <path d="M2.5 12h19M12 2.5c3 2.6 4.6 5.9 4.6 9.5s-1.6 6.9-4.6 9.5c-3-2.6-4.6-5.9-4.6-9.5s1.6-6.9 4.6-9.5zM5 6.5c1.9 1.4 4.3 2.2 7 2.2s5.1-0.8 7-2.2M5 17.5c1.9-1.4 4.3-2.2 7-2.2s5.1 0.8 7 2.2" stroke="#bbdefb" stroke-width="1.1" fill="none"/>`,
  // 摄像头：灰机身 + 蓝镜头 + 红色录制灯 + 支架底座
  camera: `
    <rect x="2" y="6" width="13.5" height="7.5" rx="2" fill="#546e7a"/>
    <rect x="2" y="6" width="13.5" height="2.4" rx="1.2" fill="#78909c"/>
    <circle cx="13.2" cy="10.2" r="2.3" fill="#42a5f5"/>
    <circle cx="13.2" cy="10.2" r="1" fill="#0d47a1"/>
    <circle cx="4.4" cy="11.2" r="0.9" fill="#ef5350"/>
    <path d="M7.5 13.5v3.2" stroke="#546e7a" stroke-width="1.8" stroke-linecap="round"/>
    <rect x="4.5" y="16.8" width="6.5" height="2" rx="1" fill="#37474f"/>
    <path d="M16.5 7.8l4.5-1.8v8.5l-4.5-1.8" fill="#ffca28"/>`,
  // 电话：话机主体 + 绿色显示屏 + 键盘
  phone: `
    <path d="M6.8 2.2h10.4l-1.2 2.4H8z" fill="#37474f"/>
    <rect x="5" y="4.6" width="14" height="17" rx="2.2" fill="#455a64"/>
    <rect x="7.3" y="6.3" width="9.4" height="4.2" rx="0.8" fill="#66bb6a"/>
    <path d="M8.6 8.4h3.2" stroke="#e8f5e9" stroke-width="0.8" stroke-linecap="round"/>
    <g fill="#b0bec5">
      <circle cx="8.6" cy="13.2" r="0.95"/><circle cx="12" cy="13.2" r="0.95"/><circle cx="15.4" cy="13.2" r="0.95"/>
      <circle cx="8.6" cy="16" r="0.95"/><circle cx="12" cy="16" r="0.95"/><circle cx="15.4" cy="16" r="0.95"/>
      <circle cx="8.6" cy="18.8" r="0.95"/><circle cx="12" cy="18.8" r="0.95"/><circle cx="15.4" cy="18.8" r="0.95"/>
    </g>`,
  // 台式机：深灰边框 + 蓝色屏幕 + 内容线 + 底座
  desktop: `
    <rect x="2.5" y="3.5" width="19" height="13" rx="1.8" fill="#455a64"/>
    <rect x="4.2" y="5.2" width="15.6" height="9.6" rx="1" fill="#1e88e5"/>
    <path d="M6 8.2l3-1.8 2.6 2.4 2.8-3.2 3.4 2.6" stroke="#bbdefb" stroke-width="1" fill="none" stroke-linejoin="round"/>
    <rect x="10.8" y="16.5" width="2.4" height="3" fill="#546e7a"/>
    <rect x="7.5" y="19.4" width="9" height="1.8" rx="0.9" fill="#455a64"/>`,
  // 笔记本：屏幕 + 键盘底座 + 触控条
  laptop: `
    <rect x="4" y="3.5" width="16" height="11" rx="1.5" fill="#455a64"/>
    <rect x="5.5" y="5" width="13" height="8" rx="0.8" fill="#1e88e5"/>
    <path d="M7.2 10.5l2.4-2.6 2 1.8 2.6-3 2.6 2.2" stroke="#bbdefb" stroke-width="0.9" fill="none" stroke-linejoin="round"/>
    <path d="M2.2 17.8h19.6l-2-2.6H4.2z" fill="#546e7a"/>
    <rect x="9.5" y="16.4" width="5" height="0.9" rx="0.45" fill="#90a4ae"/>`,
  // 打印机：机身 + 进/出纸 + 青品黄墨点 + 绿色就绪灯
  printer: `
    <rect x="7" y="2" width="10" height="4.2" rx="0.8" fill="#eceff1"/>
    <path d="M8.6 3.4h6.8" stroke="#b0bec5" stroke-width="0.7" stroke-linecap="round"/>
    <rect x="3.2" y="6.2" width="17.6" height="8.2" rx="2" fill="#546e7a"/>
    <rect x="3.2" y="6.2" width="17.6" height="2.4" rx="1.2" fill="#78909c"/>
    <circle cx="18.4" cy="11.8" r="0.95" fill="#66bb6a"/>
    <rect x="7" y="14.4" width="10" height="7" rx="0.8" fill="#eceff1"/>
    <circle cx="9.4" cy="17.9" r="0.95" fill="#29b6f6"/>
    <circle cx="12" cy="17.9" r="0.95" fill="#ef5350"/>
    <circle cx="14.6" cy="17.9" r="0.95" fill="#ffee58"/>`,
  // 其他：牛皮纸包装箱 + 封箱胶带
  other: `
    <path d="M3.5 7.6 12 3.4l8.5 4.2v8.8L12 20.6l-8.5-4.2z" fill="#a1887f"/>
    <path d="M3.5 7.6 12 11.8l8.5-4.2L12 3.4z" fill="#8d6e63"/>
    <path d="M12 11.8v8.8" stroke="#6d4c41" stroke-width="0.8"/>
    <rect x="10.9" y="3.8" width="2.2" height="16.4" rx="0.4" fill="#ffe082" opacity="0.9"/>`
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
    const cabinetKey = cabinetGroupKey(device);
    if (cabinetKey) {
      g.dataset.cabinetKey = cabinetKey;
    }

    // 坐标 0 合法（未摆放节点可为 0）：仅 null/undefined 时回退默认值
    const x = device.x ?? 100;
    const y = device.y ?? 100;
    const w = device.width ?? 200;
    const h = device.height ?? 100;
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

    // 设备类型彩色图标：左侧垂直居中，文字区右移让位
    const ICON_SIZE = 32;
    const iconG = document.createElementNS(SVG_NS, "g");
    iconG.classList.add("device-icon");
    iconG.dataset.relX = 14;
    iconG.dataset.relY = h / 2 - ICON_SIZE / 2;
    iconG.setAttribute("transform", `translate(${x + 14}, ${y + h / 2 - ICON_SIZE / 2})`);
    const iconSvg = document.createElementNS(SVG_NS, "svg");
    iconSvg.classList.add("device-type-icon");
    iconSvg.setAttribute("viewBox", "0 0 24 24");
    iconSvg.setAttribute("width", ICON_SIZE);
    iconSvg.setAttribute("height", ICON_SIZE);
    iconSvg.setAttribute("aria-hidden", "true");
    // 图标内各形状自带语义色填充，不再统一描边色
    iconSvg.innerHTML = DEVICE_ICONS[device.device_type] || DEVICE_ICONS.other;
    iconG.appendChild(iconSvg);
    g.appendChild(iconG);

    const TEXT_OFFSET_X = 16;
    const nameText = document.createElementNS(SVG_NS, "text");
    nameText.classList.add("device-name");
    nameText.textContent = device.device_name || "Unknown";
    nameText.setAttribute("x", x + w / 2 + TEXT_OFFSET_X);
    nameText.setAttribute("y", y + 22);
    centerText(nameText);
    nameText.dataset.relX = TEXT_OFFSET_X;
    nameText.dataset.relY = 22;
    g.appendChild(nameText);

    const typeText = document.createElementNS(SVG_NS, "text");
    typeText.classList.add("device-type-label");
    typeText.textContent = getDeviceTypeLabel(device.device_type) || device.device_type || "";
    typeText.setAttribute("x", x + w / 2 + TEXT_OFFSET_X);
    typeText.setAttribute("y", y + 40);
    centerText(typeText);
    typeText.dataset.relX = TEXT_OFFSET_X;
    typeText.dataset.relY = 40;
    g.appendChild(typeText);

    const ipText = document.createElementNS(SVG_NS, "text");
    ipText.classList.add("device-ip");
    ipText.textContent = device.ip_address || device.room_name || "";
    ipText.setAttribute("x", x + w / 2 + TEXT_OFFSET_X);
    ipText.setAttribute("y", y + 58);
    centerText(ipText);
    ipText.dataset.relX = TEXT_OFFSET_X;
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
    if (!sourcePos || !targetPos) {
      return null;
    }

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
    // 记录端点设备，供拖拽时增量重画相关连线
    g.dataset.sourceDevice = connection.source_device_id;
    g.dataset.targetDevice = connection.target_device_id;

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
    if (sourceLabel) {
      group.appendChild(sourceLabel);
    }

    const targetLabel = this._createConnectionLabel(
      targetAnchor.x,
      targetAnchor.y,
      connection.target_port_label || "",
      "target"
    );
    if (targetLabel) {
      group.appendChild(targetLabel);
    }
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
    centerText(badgeLabel);
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
      if (!members || members.length === 0) {
        return null;
      }
      // port_id 可能缺失：回退占位文案，避免对 null 调 slice 中断连线绘制
      const text = members
        .slice(0, 4)
        .map((m) => m.port_number || m.port_id?.slice(0, 8) || t("viz.no_port"))
        .join(", ");
      return this._createConnectionLabel(
        anchor.x,
        anchor.y,
        members.length > 4 ? `${text}…` : text,
        type
      );
    };
    const srcLabel = memberLabel(sourceAnchor, connection.source_members, "source");
    if (srcLabel) {
      group.appendChild(srcLabel);
    }
    const tgtLabel = memberLabel(targetAnchor, connection.target_members, "target");
    if (tgtLabel) {
      group.appendChild(tgtLabel);
    }
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
    centerText(label);
    node.appendChild(label);

    const indexLabel = document.createElementNS(SVG_NS, "text");
    indexLabel.classList.add("topology-outlet-index");
    indexLabel.textContent = String(index + 1);
    indexLabel.setAttribute("x", point.x);
    indexLabel.setAttribute("y", point.y + 3.5);
    centerText(indexLabel);
    node.appendChild(indexLabel);

    const typeLabel =
      hop.node_type === "patch_panel" ? t("viz.patch_panel_node") : t("viz.net_outlet_node");
    node.dataset.tooltip = `${typeLabel} ${hopName} - ${t("viz.link_order")}: ${index + 1}/${total}`;
    group.appendChild(node);
  }

  /// 在一段线路中点标注线缆标签
  _drawCableLabel(group, cable, from, to) {
    if (!cable || !cable.cable_label) {
      return;
    }
    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("topology-cable-label");
    label.textContent = cable.cable_label;
    label.setAttribute("x", (from.x + to.x) / 2);
    label.setAttribute("y", (from.y + to.y) / 2 - 6);
    centerText(label);
    group.appendChild(label);
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
      if (dx > 0) {
        anchor = { x: sourcePos.x + sourcePos.width, y: cy, dir: "right" };
      } else {
        anchor = { x: sourcePos.x, y: cy, dir: "left" };
      }
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
    if (!text) {
      return null;
    }
    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("topology-connection-label");
    label.textContent = text;
    label.setAttribute("x", x);
    label.setAttribute("y", type === "source" ? y - 10 : y + 16);
    centerText(label);
    return label;
  }

  updateConnectionPaths(deviceId) {
    // 仅重画与被拖设备相关的连线（deviceId 为空时全量重画）。
    // 重画范围扩展到直连线对端设备的全部连线（受影响设备对整体重画）：
    // 锚点错开与平行偏移计数按完整连线集合重建，避免保留连线的锚点错位重叠
    const groups = [...this.core.connectionsGroup.querySelectorAll(".topology-connection-group")];
    if (groups.length === 0) {
      return;
    }

    // 第一轮：命中与被拖设备直连的连线，并收集对端设备
    const hit = new Set();
    const neighborIds = new Set();
    groups.forEach((g) => {
      if (!deviceId || g.dataset.sourceDevice === deviceId || g.dataset.targetDevice === deviceId) {
        hit.add(g);
        if (g.dataset.sourceDevice !== deviceId) {
          neighborIds.add(g.dataset.sourceDevice);
        }
        if (g.dataset.targetDevice !== deviceId) {
          neighborIds.add(g.dataset.targetDevice);
        }
      }
    });
    if (hit.size === 0) {
      return;
    }

    // 第二轮：扩展到对端设备的全部连线，并按 DOM 顺序收集待重画连线
    const toRedraw = [];
    groups.forEach((g) => {
      if (
        hit.has(g) ||
        neighborIds.has(g.dataset.sourceDevice) ||
        neighborIds.has(g.dataset.targetDevice)
      ) {
        const conn = this._connectionsMap?.get(g.dataset.connectionId);
        if (conn) {
          toRedraw.push(conn);
          g.remove();
        }
      }
    });
    if (toRedraw.length === 0) {
      return;
    }

    // 计数器在重画前统一清零：受影响设备的全部连线均已纳入重画，
    // 锚点占用与平行偏移会按原 DOM 顺序完整重建，不会与保留连线错位
    this._connectionPairCount.clear();
    this._anchorSlots.clear();
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
