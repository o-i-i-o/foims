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

    const nameText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    nameText.setAttribute("x", x + width / 2);
    nameText.setAttribute("y", y + 30);
    nameText.dataset.relY = 30;

    const nameTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    nameTitleSpan.textContent = `${t("viz.workstation_label")}: `;

    const nameValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    nameValueSpan.textContent = workstation.name;
    nameValueSpan.className.baseVal = "workstation-name";

    nameText.appendChild(nameTitleSpan);
    nameText.appendChild(nameValueSpan);

    const ipText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    ipText.setAttribute("x", x + width / 2);
    ipText.setAttribute("y", y + 55);
    ipText.dataset.relY = 55;

    const ipTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    ipTitleSpan.textContent = "IP: ";

    const ipValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    ipValueSpan.textContent = ipAddress;
    ipValueSpan.className.baseVal = "workstation-ip";

    ipText.appendChild(ipTitleSpan);
    ipText.appendChild(ipValueSpan);

    const portText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    portText.setAttribute("x", x + width / 2);
    portText.setAttribute("y", y + 80);
    portText.dataset.relY = 80;

    const portTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    portTitleSpan.textContent = `${t("viz.port_label")}: `;

    const portValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    portValueSpan.textContent = portInfo;

    portText.appendChild(portTitleSpan);
    portText.appendChild(portValueSpan);

    const managerText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    managerText.setAttribute("x", x + width / 2);
    managerText.setAttribute("y", y + 105);
    managerText.dataset.relY = 105;

    const managerTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    managerTitleSpan.textContent = `${t("viz.manager_label")}: `;

    const managerValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    managerValueSpan.textContent = workstation.manager || t("viz.no_manager");
    managerValueSpan.className.baseVal = "workstation-manager";

    managerText.appendChild(managerTitleSpan);
    managerText.appendChild(managerValueSpan);

    const tooltipLines = [
      `${t("viz.workstation_label")}: ${workstation.name}`,
      `IP: ${ipAddress}`,
      `${t("viz.port_label")}: ${portInfo}`,
      `${t("viz.manager_label")}: ${workstation.manager || t("viz.no_manager")}`,
    ];
    if (ipManager && ipManager.network_name) {
      tooltipLines.splice(2, 0, `${t("viz.network_label")}: ${ipManager.network_name}`);
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

  drawCabinet(cabinet) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-element";
    group.dataset.id = cabinet.id;

    const x = cabinet.position.x || 50;
    const y = cabinet.position.y || 50;
    const width = cabinet.position.width || 150;
    const capacity = cabinet.capacity || 45;
    const height = cabinet.position.height || (capacity * 20 + 40);
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
      line.setAttribute("stroke", "#e0e0e0");
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

    const cabinetHeight = cabinet.position?.height || ((cabinet.capacity || 45) * 20 + 40);
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
    if (position.ipManager && position.ipManager.port_device_name && position.ipManager.port_device_number) {
      portsLabel = `${position.ipManager.port_device_name}: ${position.ipManager.port_device_number}`;
    }

    const ipAddress = position.ipManager ? position.ipManager.ip_address : t("viz.no_ip");

    group.dataset.tooltip = `${t("viz.position_label")}: ${position.name}\n${t("viz.u_position_label")}: ${startU}-${endU}\nIP: ${ipAddress}\n${portsLabel}`;

    group.appendChild(rect);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  drawDoor() {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "door-element";
    group.dataset.elementType = "door";

    const x = 50;
    const y = 100;
    const width = 40;
    const height = 80;
    
    const doorFrame = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    doorFrame.setAttribute("x", x);
    doorFrame.setAttribute("y", y);
    doorFrame.setAttribute("width", width);
    doorFrame.setAttribute("height", height);
    doorFrame.setAttribute("fill", "#d1d5db");
    doorFrame.setAttribute("stroke", "#6b7280");
    doorFrame.setAttribute("stroke-width", "2");
    
    const doorHandle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    doorHandle.setAttribute("cx", x + width - 10);
    doorHandle.setAttribute("cy", y + height / 2);
    doorHandle.setAttribute("r", 3);
    doorHandle.setAttribute("fill", "#6b7280");
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
