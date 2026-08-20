import { t } from "../../utils/i18n.js";

export class SVGRenderer {
  constructor(core) {
    this.core = core;
    this.elementsGroup = core.elementsGroup;
  }

  drawWorkstation(workstation) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "workstation-element";
    group.dataset.id = workstation.id;

    const x = workstation.position.x || 100;
    const y = workstation.position.y || 100;
    const width = workstation.position.width || 240;
    const height = workstation.position.height || 120;

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", width);
    rect.setAttribute("height", height);

    const ipManager = workstation.ipManager || null;
    const ipAddress = ipManager ? ipManager.ip_address : t("viz.no_ip");
    let portInfo = t("viz.no_port");
    if (ipManager && ipManager.port_device_name && ipManager.port_device_number) {
      portInfo = `${ipManager.port_device_name}: ${ipManager.port_device_number}`;
    }

    const statusClass =
      ipManager && ipManager.status ? `status-${ipManager.status}` : "status-unknown";
    group.classList.add(statusClass);

    // 工位卡片四行标签文本（名称/IP/端口/负责人），统一走 labelText helper
    const lines = [
      {
        relY: 30,
        label: `${t("viz.workstation_label")}: `,
        value: workstation.name,
        valueClass: "workstation-name"
      },
      { relY: 55, label: "IP: ", value: ipAddress, valueClass: "workstation-ip" },
      { relY: 80, label: `${t("viz.port_label")}: `, value: portInfo },
      {
        relY: 105,
        label: `${t("viz.manager_label")}: `,
        value: workstation.manager || t("viz.no_manager"),
        valueClass: "workstation-manager"
      }
    ];
    const textElements = lines.map((line) =>
      this.labelText(x + width / 2, y + line.relY, line.relY, line)
    );

    const tooltipLines = [
      `${t("viz.workstation_label")}: ${workstation.name}`,
      `IP: ${ipAddress}`,
      `${t("viz.port_label")}: ${portInfo}`,
      `${t("viz.manager_label")}: ${workstation.manager || t("viz.no_manager")}`
    ];
    if (ipManager && (ipManager.network_name || ipManager.network_region)) {
      const netParts = [ipManager.network_region, ipManager.network_name]
        .filter(Boolean)
        .join(" / ");
      tooltipLines.splice(2, 0, `${t("viz.network_label")}: ${netParts}`);
    }
    group.dataset.tooltip = tooltipLines.join("\n");

    group.appendChild(rect);
    for (const textEl of textElements) {
      group.appendChild(textEl);
    }
    this.elementsGroup.appendChild(group);

    return group;
  }

  /**
   * 创建一行「标签: 值」SVG 文本（label 与 value 各占一个 tspan）。
   * @param {number} x 文本 x 坐标
   * @param {number} y 文本 y 坐标
   * @param {number} relY 相对节点顶部的 y 偏移（供拖拽缩放重排使用）
   * @param {Object} line { label, value, valueClass }
   */
  labelText(x, y, relY, { label, value, valueClass }) {
    const SVG_NS = "http://www.w3.org/2000/svg";
    const text = document.createElementNS(SVG_NS, "text");
    text.setAttribute("x", x);
    text.setAttribute("y", y);
    text.dataset.relY = relY;

    const titleSpan = document.createElementNS(SVG_NS, "tspan");
    titleSpan.textContent = label;

    const valueSpan = document.createElementNS(SVG_NS, "tspan");
    valueSpan.textContent = value;
    if (valueClass) {
      valueSpan.className.baseVal = valueClass;
    }

    text.appendChild(titleSpan);
    text.appendChild(valueSpan);
    return text;
  }

  drawCabinet(cabinet) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-element";
    group.dataset.id = cabinet.id;

    const x = cabinet.position.x || 50;
    const y = cabinet.position.y || 50;
    const width = cabinet.position.width || 150;
    const capacity = cabinet.capacity || 45;
    const height = cabinet.position.height || capacity * 20 + 40;
    const uHeight = (height - 40) / capacity;

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", width);
    rect.setAttribute("height", height);

    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", x + width / 2);
    text.setAttribute("y", y + 20);
    text.dataset.relY = 20;
    text.textContent = cabinet.name;
    group.dataset.tooltip = `${t("viz.cabinet_label")}: ${cabinet.name}\n${t("viz.capacity_label")}: ${cabinet.capacity || 42}${t("viz.u_unit")}`;

    group.appendChild(rect);
    this.drawUMarks(cabinet, group, x, y, width, capacity, uHeight);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  drawUMarks(cabinet, group, baseX, baseY, width, capacity, uHeight) {
    const startY = baseY + 40;

    for (let i = 1; i <= capacity; i++) {
      const y = startY + (capacity - i) * uHeight;

      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", baseX + 10);
      line.setAttribute("y1", y);
      line.setAttribute("x2", baseX + width - 10);
      line.setAttribute("y2", y);
      line.style.stroke = "var(--color-border-light)";
      line.setAttribute("stroke-width", "0.5");
      line.dataset.relX1 = 10;
      line.dataset.relY1 = y - baseY;
      line.dataset.relX2 = width - 10;
      line.dataset.relY2 = y - baseY;
      group.appendChild(line);

      if (i % 5 === 0 || i === 1 || i === capacity) {
        const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
        text.setAttribute("x", baseX + 5);
        text.setAttribute("y", y + uHeight / 2);
        text.dataset.relX = 5;
        text.dataset.relY = y - baseY + uHeight / 2;
        text.textContent = i;
        text.className.baseVal = "u-mark";
        group.appendChild(text);
      }
    }
  }

  drawCabinetPosition(position, cabinet) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-position-element";
    group.dataset.id = position.id;
    group.dataset.cabinetId = cabinet.id;

    const cabinetHeight = cabinet.position?.height || (cabinet.capacity || 45) * 20 + 40;
    const capacity = cabinet.capacity || 45;
    const uHeight = (cabinetHeight - 40) / capacity;

    const startU = position.start_u || 1;
    const endU = position.end_u || startU;
    const height = Math.max((endU - startU + 1) * uHeight, uHeight);

    const cabinetY = cabinet.position?.y || 50;
    const cabinetX = cabinet.position?.x || 50;

    const startY = cabinetY + 40;
    const y = startY + (capacity - endU) * uHeight;
    const x = cabinetX + 20;

    group.dataset.relX = 20;
    group.dataset.relY = y - cabinetY;

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", 110);
    rect.setAttribute("height", height);

    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", x + 55);
    text.setAttribute("y", y + height / 2 + 4);
    text.dataset.relX = 55;
    text.dataset.relY = height / 2 + 4;
    text.textContent = position.name;

    let portsLabel = t("viz.no_port");
    if (
      position.ipManager &&
      position.ipManager.port_device_name &&
      position.ipManager.port_device_number
    ) {
      portsLabel = `${position.ipManager.port_device_name}: ${position.ipManager.port_device_number}`;
    }

    const ipAddress = position.ipManager ? position.ipManager.ip_address : t("viz.no_ip");

    let tooltip = `${t("viz.position_label")}: ${position.name}\n${t("viz.u_position_label")}: ${startU}-${endU}\nIP: ${ipAddress}`;
    if (position.ipManager) {
      const netParts = [position.ipManager.network_region, position.ipManager.network_name]
        .filter(Boolean)
        .join(" / ");
      if (netParts) {
        tooltip += `\n${t("viz.network_label")}: ${netParts}`;
      }
    }
    tooltip += `\n${portsLabel}`;

    group.dataset.tooltip = tooltip;

    group.appendChild(rect);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  /**
   * 绘制门元素。
   * @param {string|null} id element_layouts 已保存记录的主键；未保存过的门
   *   生成随机 UUID 仅作 DOM 标识（后端按 room_id+element_type 落库，不使用该 id）
   */
  drawDoor(id = null) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "door-element";
    group.dataset.elementType = "door";
    group.dataset.id = id || crypto.randomUUID();

    const x = 50;
    const y = 100;
    const width = 40;
    const height = 80;

    const doorFrame = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    doorFrame.setAttribute("x", x);
    doorFrame.setAttribute("y", y);
    doorFrame.setAttribute("width", width);
    doorFrame.setAttribute("height", height);
    doorFrame.style.fill = "var(--color-bg-secondary)";
    doorFrame.style.stroke = "var(--color-text-secondary)";
    doorFrame.setAttribute("stroke-width", "2");

    const doorHandle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    doorHandle.setAttribute("cx", x + width - 10);
    doorHandle.setAttribute("cy", y + height / 2);
    doorHandle.setAttribute("r", 3);
    doorHandle.style.fill = "var(--color-text-secondary)";
    doorHandle.dataset.relCx = width - 10;
    doorHandle.dataset.relCy = height / 2;

    const doorLabel = document.createElementNS("http://www.w3.org/2000/svg", "text");
    doorLabel.setAttribute("x", x + width / 2);
    doorLabel.setAttribute("y", y - 10);
    doorLabel.dataset.relX = 0;
    doorLabel.dataset.relY = -10;
    doorLabel.textContent = t("viz.door");

    group.dataset.tooltip = t("viz.room_entrance_ref");

    group.appendChild(doorFrame);
    group.appendChild(doorHandle);
    group.appendChild(doorLabel);
    this.elementsGroup.appendChild(group);

    return group;
  }

  clearElements() {
    this.elementsGroup.innerHTML = "";
  }
}
