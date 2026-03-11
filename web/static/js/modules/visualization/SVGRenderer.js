export class SVGRenderer {
  constructor(core) {
    this.core = core;
    this.elementsGroup = core.elementsGroup;
  }

  drawWorkstation(workstation) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "workstation-element";
    group.dataset.id = workstation.id;

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", workstation.position.x || 100);
    rect.setAttribute("y", workstation.position.y || 100);
    rect.setAttribute("width", workstation.position.width || 160);
    rect.setAttribute("height", workstation.position.height || 160);

    const ipManager = workstation.ipManager || null;
    const ipAddress = ipManager ? ipManager.ip_address : "无IP";
    let portInfo = "无端口";
    if (ipManager && ipManager.switch_name && ipManager.switch_port_number) {
      portInfo = `${ipManager.switch_name}: ${ipManager.switch_port_number}`;
    }
    
    const statusClass =
      ipManager && ipManager.status ? `status-${ipManager.status}` : "status-unknown";
    group.classList.add(statusClass);

    const x = workstation.position.x || 100;
    const y = workstation.position.y || 100;
    const width = workstation.position.width || 160;
    const height = workstation.position.height || 160;

    const nameText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    nameText.setAttribute("x", x + width / 2);
    nameText.setAttribute("y", y + 40);

    const nameTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    nameTitleSpan.textContent = "工位: ";

    const nameValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    nameValueSpan.textContent = workstation.name;
    nameValueSpan.className.baseVal = "workstation-name";

    nameText.appendChild(nameTitleSpan);
    nameText.appendChild(nameValueSpan);

    const ipText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    ipText.setAttribute("x", x + width / 2);
    ipText.setAttribute("y", y + 70);

    const ipTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    ipTitleSpan.textContent = "IP: ";

    const ipValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    ipValueSpan.textContent = ipAddress;
    ipValueSpan.className.baseVal = "workstation-ip";

    ipText.appendChild(ipTitleSpan);
    ipText.appendChild(ipValueSpan);

    const portText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    portText.setAttribute("x", x + width / 2);
    portText.setAttribute("y", y + 100);

    const portTitleSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    portTitleSpan.textContent = "端口: ";

    const portValueSpan = document.createElementNS("http://www.w3.org/2000/svg", "tspan");
    portValueSpan.textContent = portInfo;

    portText.appendChild(portTitleSpan);
    portText.appendChild(portValueSpan);

    const managerText = document.createElementNS("http://www.w3.org/2000/svg", "text");
    managerText.setAttribute("x", x + width / 2);
    managerText.setAttribute("y", y + 130);

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

  drawCabinet(cabinet) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-element";
    group.dataset.id = cabinet.id;

    const x = cabinet.position.x || 50;
    const y = cabinet.position.y || 50;
    const width = cabinet.position.width || 150;
    const capacity = cabinet.capacity || 45;
    const height = cabinet.position.height || (capacity * 20 + 40);

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", width);
    rect.setAttribute("height", height);

    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", x + width / 2);
    text.setAttribute("y", y + 20);
    text.textContent = cabinet.name;
    group.dataset.tooltip = `机柜: ${cabinet.name}\n容量: ${cabinet.capacity || 42}U`;

    group.appendChild(rect);
    this.drawUMarks(cabinet, group);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  drawUMarks(cabinet, group) {
    const uHeight = 20;
    const startY = (cabinet.position.y || 50) + 40;
    const width = cabinet.position.width || 150;
    const capacity = cabinet.capacity || 45;

    for (let i = 1; i <= capacity; i++) {
      const y = startY + (capacity - i) * uHeight;

      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", (cabinet.position.x || 50) + 10);
      line.setAttribute("y1", y);
      line.setAttribute("x2", (cabinet.position.x || 50) + width - 10);
      line.setAttribute("y2", y);
      line.setAttribute("stroke", "#e0e0e0");
      line.setAttribute("stroke-width", "0.5");
      group.appendChild(line);

      if (i % 5 === 0 || i === 1 || i === capacity) {
        const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
        text.setAttribute("x", (cabinet.position.x || 50) + 5);
        text.setAttribute("y", y + uHeight / 2);
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

    const uHeight = 20;
    const startU = position.start_u || 1;
    const endU = position.end_u || startU;
    const height = Math.max((endU - startU + 1) * uHeight, uHeight);

    const cabinetY = cabinet.position?.y || 50;
    const cabinetX = cabinet.position?.x || 50;
    const capacity = cabinet.capacity || 45;

    const startY = cabinetY + 40;
    const y = startY + (capacity - endU) * uHeight;

    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", cabinetX + 20);
    rect.setAttribute("y", y);
    rect.setAttribute("width", 110);
    rect.setAttribute("height", height);

    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", cabinetX + 75);
    text.setAttribute("y", y + height / 2 + 4);
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

  drawDoor() {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "door-element";
    group.dataset.id = "00000000-0000-0000-0000-000000000001";
    
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
    
    const doorLabel = document.createElementNS("http://www.w3.org/2000/svg", "text");
    doorLabel.setAttribute("x", x + width / 2);
    doorLabel.setAttribute("y", y - 10);
    doorLabel.textContent = "门";
    
    group.dataset.tooltip = "房间入口参考点";
    
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
