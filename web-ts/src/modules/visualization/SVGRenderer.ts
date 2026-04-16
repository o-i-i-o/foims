// SVG Renderer - Handles drawing of SVG elements
import type { SVGCore } from "./SVGCore.js";

interface Workstation {
  id: string;
  name: string;
  manager?: string;
  position?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  ipManager?: {
    ip_address?: string;
    status?: string;
    switch_name?: string;
    switch_port_number?: string;
    network_name?: string;
  } | null;
}

interface Cabinet {
  id: string;
  name: string;
  capacity?: number;
  position?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
}

interface CabinetPosition {
  id: string;
  name: string;
  start_u: number;
  end_u: number;
  ipManager?: {
    ip_address?: string;
    switch_name?: string;
    switch_port_number?: string;
  } | null;
}

export class SVGRenderer {
  private elementsGroup: SVGGElement;

  constructor(core: SVGCore) {
    this.elementsGroup = core.elementsGroup;
  }

  drawWorkstation(workstation: Workstation): SVGGElement {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "workstation-element";
    group.dataset.id = workstation.id;

    const x = workstation.position?.x || 100;
    const y = workstation.position?.y || 100;
    const width = workstation.position?.width || 160;
    const height = workstation.position?.height || 160;

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", String(x));
    rect.setAttribute("y", String(y));
    rect.setAttribute("width", String(width));
    rect.setAttribute("height", String(height));

    const ipManager = workstation.ipManager || null;
    const ipAddress = ipManager ? ipManager.ip_address : "无IP";
    let portInfo = "无端口";
    if (ipManager && ipManager.switch_name && ipManager.switch_port_number) {
      portInfo = `${ipManager.switch_name}: ${ipManager.switch_port_number}`;
    }

    const statusClass = ipManager && ipManager.status ? `status-${ipManager.status}` : "status-unknown";
    group.classList.add(statusClass);

    const nameText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    nameText.setAttribute("x", String(x + width / 2));
    nameText.setAttribute("y", String(y + 40));
    (nameText as SVGElement).dataset.relY = "40";

    const nameTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    nameTitleSpan.textContent = "工位: ";

    const nameValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    nameValueSpan.textContent = workstation.name;
    nameValueSpan.className.baseVal = "workstation-name";

    nameText.appendChild(nameTitleSpan);
    nameText.appendChild(nameValueSpan);

    const ipText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    ipText.setAttribute("x", String(x + width / 2));
    ipText.setAttribute("y", String(y + 70));
    (ipText as SVGElement).dataset.relY = "70";

    const ipTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    ipTitleSpan.textContent = "IP: ";

    const ipValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    ipValueSpan.textContent = ipAddress || "无IP";
    ipValueSpan.className.baseVal = "workstation-ip";

    ipText.appendChild(ipTitleSpan);
    ipText.appendChild(ipValueSpan);

    const portText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    portText.setAttribute("x", String(x + width / 2));
    portText.setAttribute("y", String(y + 100));
    (portText as SVGElement).dataset.relY = "100";

    const portTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    portTitleSpan.textContent = "端口: ";

    const portValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    portValueSpan.textContent = portInfo;

    portText.appendChild(portTitleSpan);
    portText.appendChild(portValueSpan);

    const managerText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    managerText.setAttribute("x", String(x + width / 2));
    managerText.setAttribute("y", String(y + 130));
    (managerText as SVGElement).dataset.relY = "130";

    const managerTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    managerTitleSpan.textContent = "管理人: ";

    const managerValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    managerValueSpan.textContent = workstation.manager || "无管理人";
    managerValueSpan.className.baseVal = "workstation-manager";

    managerText.appendChild(managerTitleSpan);
    managerText.appendChild(managerValueSpan);

    const tooltipLines = [
      `工位: ${workstation.name}`,
      `IP: ${ipAddress}`,
      `端口: ${portInfo}`,
      `管理人: ${workstation.manager || "无管理人"}`,
    ];
    if (ipManager && ipManager.network_name) {
      tooltipLines.splice(2, 0, `网络: ${ipManager.network_name}`);
    }
    group.dataset.tooltip = tooltipLines.join("\n");

    group.appendChild(rect);
    group.appendChild(nameText);
    group.appendChild(ipText);
    group.appendChild(portText);
    group.appendChild(managerText);
    this.elementsGroup.appendChild(group);

    return group;
  }

  drawCabinet(cabinet: Cabinet): SVGGElement {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-element";
    group.dataset.id = cabinet.id;

    const x = cabinet.position?.x || 50;
    const y = cabinet.position?.y || 50;
    const width = cabinet.position?.width || 150;
    const capacity = cabinet.capacity || 45;
    const height = cabinet.position?.height || capacity * 20 + 40;

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", String(x));
    rect.setAttribute("y", String(y));
    rect.setAttribute("width", String(width));
    rect.setAttribute("height", String(height));

    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", String(x + width / 2));
    text.setAttribute("y", String(y + 20));
    (text as SVGElement).dataset.relY = "20";
    text.textContent = cabinet.name;
    group.dataset.tooltip = `机柜: ${cabinet.name}\n容量: ${cabinet.capacity || 42}U`;

    group.appendChild(rect);
    this.drawUMarks(cabinet, group, x, y, width, capacity);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  private drawUMarks(_cabinet: Cabinet, group: SVGGElement, baseX: number, baseY: number, width: number, capacity: number): void {
    const uHeight = 20;
    const startY = baseY + 40;

    for (let i = 1; i <= capacity; i++) {
      const y = startY + (capacity - i) * uHeight;

      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", String(baseX + 10));
      line.setAttribute("y1", String(y));
      line.setAttribute("x2", String(baseX + width - 10));
      line.setAttribute("y2", String(y));
      line.setAttribute("stroke", "#e0e0e0");
      line.setAttribute("stroke-width", "0.5");
      (line as SVGElement).dataset.relX1 = "10";
      (line as SVGElement).dataset.relY1 = String(y - baseY);
      (line as SVGElement).dataset.relX2 = String(width - 10);
      (line as SVGElement).dataset.relY2 = String(y - baseY);
      group.appendChild(line);

      if (i % 5 === 0 || i === 1 || i === capacity) {
        const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
        text.setAttribute("x", String(baseX + 5));
        text.setAttribute("y", String(y + uHeight / 2));
        (text as SVGElement).dataset.relX = "5";
        (text as SVGElement).dataset.relY = String(y - baseY + uHeight / 2);
        text.textContent = String(i);
        text.className.baseVal = "u-mark";
        group.appendChild(text);
      }
    }
  }

  drawCabinetPosition(position: CabinetPosition, cabinet: Cabinet): SVGGElement {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-position-element";
    group.dataset.id = position.id;
    group.dataset.cabinetId = cabinet.id;

    const uHeight = 20;
    const startU = position.start_u || 1;
    const endU = position.end_u || startU;
    const height = Math.max((endU - startU + 1) * uHeight, uHeight);

    const cabinetY = cabinet.position?.y || 50;
    const cabinetX = cabinet.position?.x || 50;
    const capacity = cabinet.capacity || 45;

    const startY = cabinetY + 40;
    const y = startY + (capacity - endU) * uHeight;
    const x = cabinetX + 20;

    group.dataset.relX = "20";
    group.dataset.relY = String(y - cabinetY);

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", String(x));
    rect.setAttribute("y", String(y));
    rect.setAttribute("width", "110");
    rect.setAttribute("height", String(height));

    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", String(x + 55));
    text.setAttribute("y", String(y + height / 2 + 4));
    (text as SVGElement).dataset.relX = "55";
    (text as SVGElement).dataset.relY = String(height / 2 + 4);
    text.textContent = position.name;

    let portsLabel = "无端口";
    if (position.ipManager && position.ipManager.switch_name && position.ipManager.switch_port_number) {
      portsLabel = `${position.ipManager.switch_name}: ${position.ipManager.switch_port_number}`;
    }

    const ipAddress = position.ipManager ? position.ipManager.ip_address : "无IP";

    group.dataset.tooltip = `机位: ${position.name}\nU位: ${startU}-${endU}\nIP: ${ipAddress}\n${portsLabel}`;

    group.appendChild(rect);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  drawDoor(): SVGGElement {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "door-element";
    group.dataset.id = "00000000-0000-0000-0000-000000000001";

    const x = 50;
    const y = 100;
    const width = 40;
    const height = 80;

    const doorFrame = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    doorFrame.setAttribute("x", String(x));
    doorFrame.setAttribute("y", String(y));
    doorFrame.setAttribute("width", String(width));
    doorFrame.setAttribute("height", String(height));
    doorFrame.setAttribute("fill", "#d1d5db");
    doorFrame.setAttribute("stroke", "#6b7280");
    doorFrame.setAttribute("stroke-width", "2");

    const doorHandle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    doorHandle.setAttribute("cx", String(x + width - 10));
    doorHandle.setAttribute("cy", String(y + height / 2));
    doorHandle.setAttribute("r", "3");
    doorHandle.setAttribute("fill", "#6b7280");
    (doorHandle as SVGElement).dataset.relCx = String(width - 10);
    (doorHandle as SVGElement).dataset.relCy = String(height / 2);

    const doorLabel = document.createElementNS("http://www.w3.org/2000/svg", "text");
    doorLabel.setAttribute("x", String(x + width / 2));
    doorLabel.setAttribute("y", String(y - 10));
    (doorLabel as SVGElement).dataset.relX = "0";
    (doorLabel as SVGElement).dataset.relY = "-10";
    doorLabel.textContent = "门";

    group.dataset.tooltip = "房间入口参考点";

    group.appendChild(doorFrame);
    group.appendChild(doorHandle);
    group.appendChild(doorLabel);
    this.elementsGroup.appendChild(group);

    return group;
  }

  clearElements(): void {
    this.elementsGroup.innerHTML = "";
  }
}
