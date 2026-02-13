import { apiGet, getAccessToken } from "../utils/apiClient.js";
import { showToast, showMessage } from "../utils/ui.js";

// 生成UUID的工具函数
function generateUUID() {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

export class SVGVisualization {
  constructor(containerId, type, callbacks = {}) {
    this.container = document.getElementById(containerId);
    this.type = type; // 'workstation' 或 'cabinet'
    this.svg = null;
    this.elementsGroup = null;
    this.selectedElement = null;
    this.isDragging = false;
    this.dragStart = { x: 0, y: 0 };
    this.currentRoomId = null;
    this.currentNetworkRegionId = null;
    this.currentCabinetId = null;
    this.gridSize = 10;
    this.tooltip = this.createTooltip();
    this.callbacks = callbacks;

    // 初始化SVG元素
    this.initSVG();
  }

  // 初始化SVG元素
  initSVG() {
    // 清空容器
    this.container.innerHTML = "";

    // 创建SVG元素
    this.svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    this.svg.className.baseVal = "visualization-svg";
    this.svg.setAttribute("width", "100%");
    this.svg.setAttribute("height", "100%");
    // 初始viewBox设置为较大的值，后续会根据实际内容调整
    this.svg.setAttribute("viewBox", "0 0 2000 2000");

    // 创建网格背景
    this.createGridBackground();

    // 创建元素组
    this.elementsGroup = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "g",
    );
    this.svg.appendChild(this.elementsGroup);

    // 添加SVG到容器
    this.container.appendChild(this.svg);

    // 设置容器样式，确保可以滚动
    this.container.style.overflow = "auto";
    this.container.style.maxHeight = "800px";
    this.container.style.border = "1px solid #ddd";
    this.container.style.borderRadius = "4px";

    // 初始化事件监听器
    this.initEventListeners();
  }

  // 创建网格背景
  createGridBackground() {
    // 创建defs元素
    const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");

    // 创建网格模式
    const pattern = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "pattern",
    );
    const gridId = `grid-${this.type}`;
    pattern.setAttribute("id", gridId);
    pattern.setAttribute("width", "50");
    pattern.setAttribute("height", "50");
    pattern.setAttribute("patternUnits", "userSpaceOnUse");

    // 创建网格线
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", "M 50 0 L 0 0 0 50");
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "var(--border-light)");
    path.setAttribute("stroke-width", "0.5");

    // 组装元素
    pattern.appendChild(path);
    defs.appendChild(pattern);
    this.svg.appendChild(defs);

    // 创建背景矩形
    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("width", "100%");
    rect.setAttribute("height", "100%");
    rect.setAttribute("fill", `url(#${gridId})`);
    this.svg.appendChild(rect);
  }

  // 初始化事件监听器
  initEventListeners() {
    // 鼠标事件
    this.svg.addEventListener("mousedown", this.handleMouseDown.bind(this));
    this.svg.addEventListener("mousemove", this.handleMouseMove.bind(this));
    this.svg.addEventListener("mouseup", this.handleMouseUp.bind(this));
    this.svg.addEventListener("mouseleave", () => {
      this.handleMouseUp();
      this.hideTooltip();
    });

    // 点击空白处清除选择
    this.svg.addEventListener("click", (e) => {
      const isElement = e.target.closest("[data-id]");
      if (!isElement) {
        this.clearSelection();
      }
    });

    // 右键菜单事件已移除，不再支持右键删除工位
  }

  // 处理鼠标按下事件
  handleMouseDown(e) {
    if (e.button !== 0) return;
    // 检查是否点击了元素
    const target = e.target.closest("[data-id]");
    if (target) {
      this.isDragging = true;
      this.hasMoved = false; // 重置移动标记
      this.selectedElement = target;
      this.dragStart = this.getSvgCoordinates(e);

      // 添加选中样式
      this.selectElement(target);
    }
  }

  // 处理鼠标移动事件
  handleMouseMove(e) {
    if (this.isDragging && this.selectedElement) {
      const currentPos = this.getSvgCoordinates(e);
      const dx = currentPos.x - this.dragStart.x;
      const dy = currentPos.y - this.dragStart.y;

      // 只有当移动距离超过阈值时才认为是拖拽
      if (Math.abs(dx) > 2 || Math.abs(dy) > 2) {
        this.hasMoved = true;
        // 更新元素位置
        this.updateElementPosition(this.selectedElement, dx, dy);

        // 更新拖拽起点
        this.dragStart = currentPos;
        this.hideTooltip();
      }
      return;
    }

    this.updateTooltip(e);
  }

  // 处理鼠标释放事件
  handleMouseUp(e) {
    if (this.isDragging && !this.hasMoved && this.selectedElement) {
      // 如果没有移动，则视为点击事件
      this.handleElementClick(this.selectedElement);
    }

    this.isDragging = false;
    if (this.selectedElement) {
      this.snapElementToGrid(this.selectedElement);
    }
  }

  // 处理元素点击事件
  handleElementClick(element) {
    const id = element.dataset.id;
    const elementType = element.classList.contains("workstation-element")
      ? "workstation"
      : element.classList.contains("cabinet-element")
        ? "cabinet"
        : element.classList.contains("cabinet-position-element")
          ? "cabinet-position"
          : "";

    if (elementType === "workstation" && this.callbacks.onEditWorkstation) {
      this.callbacks.onEditWorkstation(id);
    } else if (elementType === "cabinet" && this.callbacks.onEditCabinet) {
      this.callbacks.onEditCabinet(id);
    } else if (elementType === "cabinet-position" && this.callbacks.onEditCabinetPosition) {
      this.callbacks.onEditCabinetPosition(id);
    }
  }

  // 获取SVG坐标
  getSvgCoordinates(e) {
    const pt = this.svg.createSVGPoint();
    const rect = this.svg.getBoundingClientRect();
    pt.x = e.clientX - rect.left;
    pt.y = e.clientY - rect.top;
    const ctm = this.svg.getScreenCTM().inverse();
    return pt.matrixTransform(ctm);
  }

  // 选择元素
  selectElement(element) {
    // 清除之前的选择
    this.clearSelection();

    // 选择当前元素
    this.selectedElement = element;
    element.classList.add("selected");
  }

  // 清除选择
  clearSelection() {
    const selected = this.elementsGroup.querySelector(".selected");
    if (selected) {
      selected.classList.remove("selected");
    }
    this.selectedElement = null;
  }

  // 更新元素位置
  updateElementPosition(element, dx, dy) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const x = parseFloat(rect.getAttribute("x")) + dx;
    const y = parseFloat(rect.getAttribute("y")) + dy;

    rect.setAttribute("x", x);
    rect.setAttribute("y", y);

    // 更新所有文本位置
    const texts = element.querySelectorAll("text");
    texts.forEach((text) => {
      const textX = parseFloat(text.getAttribute("x")) + dx;
      const textY = parseFloat(text.getAttribute("y")) + dy;
      text.setAttribute("x", textX);
      text.setAttribute("y", textY);
    });

    // 更新所有线条位置（用于机柜U位标记）
    const lines = element.querySelectorAll("line");
    lines.forEach((line) => {
      const x1 = parseFloat(line.getAttribute("x1")) + dx;
      const y1 = parseFloat(line.getAttribute("y1")) + dy;
      const x2 = parseFloat(line.getAttribute("x2")) + dx;
      const y2 = parseFloat(line.getAttribute("y2")) + dy;
      line.setAttribute("x1", x1);
      line.setAttribute("y1", y1);
      line.setAttribute("x2", x2);
      line.setAttribute("y2", y2);
    });

    // 更新所有子矩形位置（用于机位等嵌套元素）
    const childRects = element.querySelectorAll("rect:not(:first-child)");
    childRects.forEach((childRect) => {
      const childX = parseFloat(childRect.getAttribute("x")) + dx;
      const childY = parseFloat(childRect.getAttribute("y")) + dy;
      childRect.setAttribute("x", childX);
      childRect.setAttribute("y", childY);
    });

    // 更新门把手位置（如果是门图标）
    if (element.classList.contains("door-element")) {
      const doorHandle = element.querySelector("circle");
      if (doorHandle) {
        const handleX = parseFloat(doorHandle.getAttribute("cx")) + dx;
        const handleY = parseFloat(doorHandle.getAttribute("cy")) + dy;
        doorHandle.setAttribute("cx", handleX);
        doorHandle.setAttribute("cy", handleY);
      }
    }

    // 如果拖动的是机柜，同时更新该机柜内所有机位的位置
    if (element.classList.contains("cabinet-element")) {
      const cabinetId = element.dataset.id;
      const positions = this.elementsGroup.querySelectorAll(".cabinet-position-element");
      positions.forEach((position) => {
        // 这里需要一种方法来确定机位属于哪个机柜
        // 由于当前代码没有直接关联，我们可以通过位置关系来判断
        const positionRect = position.querySelector("rect");
        if (positionRect) {
          const posX = parseFloat(positionRect.getAttribute("x"));
          const posY = parseFloat(positionRect.getAttribute("y"));
          // 简单判断：如果机位在机柜附近，则认为是该机柜的机位
          if (Math.abs(posX - (x + 15)) < 10 && Math.abs(posY - (y + 40)) < 10) {
            const newPosX = posX + dx;
            const newPosY = posY + dy;
            positionRect.setAttribute("x", newPosX);
            positionRect.setAttribute("y", newPosY);
            // 更新机位文本位置
            const positionTexts = position.querySelectorAll("text");
            positionTexts.forEach((text) => {
              const textX = parseFloat(text.getAttribute("x")) + dx;
              const textY = parseFloat(text.getAttribute("y")) + dy;
              text.setAttribute("x", textX);
              text.setAttribute("y", textY);
            });
          }
        }
      });
    }
  }

  snapElementToGrid(element) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const grid = this.gridSize;
    if (!grid) return;

    const x = parseFloat(rect.getAttribute("x"));
    const y = parseFloat(rect.getAttribute("y"));
    const snappedX = Math.round(x / grid) * grid;
    const snappedY = Math.round(y / grid) * grid;

    const dx = snappedX - x;
    const dy = snappedY - y;

    if (dx !== 0 || dy !== 0) {
      this.updateElementPosition(element, dx, dy);
    }
  }

  createTooltip() {
    const tooltipId = `visualization-tooltip-${this.type}`;
    const existing = document.getElementById(tooltipId);
    if (existing) return existing;

    const tooltip = document.createElement("div");
    tooltip.id = tooltipId;
    tooltip.className = "tooltip";
    document.body.appendChild(tooltip);
    return tooltip;
  }

  updateTooltip(e) {
    if (!this.tooltip) return;
    const target = e.target.closest("[data-tooltip]");
    if (!target || !target.dataset.tooltip) {
      this.hideTooltip();
      return;
    }

    this.tooltip.textContent = target.dataset.tooltip;
    this.tooltip.style.left = `${e.clientX + 12}px`;
    this.tooltip.style.top = `${e.clientY + 12}px`;
    this.tooltip.classList.add("visible");
  }

  hideTooltip() {
    if (this.tooltip) {
      this.tooltip.classList.remove("visible");
    }
  }

  // 绘制工位
  drawWorkstation(workstation) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "workstation-element";
    group.dataset.id = workstation.id;

    // 工位矩形 - 大小翻倍为160x160
    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", workstation.position.x || 100);
    rect.setAttribute("y", workstation.position.y || 100);
    rect.setAttribute("width", workstation.position.width || 160);
    rect.setAttribute("height", workstation.position.height || 160);

    // 获取端口号（显示所有端口）
    const portInfo =
      workstation.ports && workstation.ports.length > 0
        ? workstation.ports.map(port => port.port_number || "未知端口").join(", ")
        : "无端口";
    const ipManager = workstation.ipManager || null;
    const ipAddress = ipManager ? ipManager.ip_address : "无IP";
    const statusClass =
      ipManager && ipManager.status ? `status-${ipManager.status}` : "status-unknown";
    group.classList.add(statusClass);

    const x = workstation.position.x || 100;
    const y = workstation.position.y || 100;
    const width = workstation.position.width || 160;
    const height = workstation.position.height || 160;

    // 工位名称
    const nameText = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    nameText.setAttribute("x", x + width / 2);
    nameText.setAttribute("y", y + 40);

    const nameTitleSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
    nameTitleSpan.textContent = "工位: ";

    const nameValueSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
    nameValueSpan.textContent = workstation.name;
    nameValueSpan.className.baseVal = "workstation-name";

    nameText.appendChild(nameTitleSpan);
    nameText.appendChild(nameValueSpan);

    // IP地址
    const ipText = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    ipText.setAttribute("x", x + width / 2);
    ipText.setAttribute("y", y + 70);

    const ipTitleSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
    ipTitleSpan.textContent = "IP: ";

    const ipValueSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
    ipValueSpan.textContent = ipAddress;
    ipValueSpan.className.baseVal = "workstation-ip";

    ipText.appendChild(ipTitleSpan);
    ipText.appendChild(ipValueSpan);

    // 端口号
    const portText = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    portText.setAttribute("x", x + width / 2);
    portText.setAttribute("y", y + 100);

    const portTitleSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
    portTitleSpan.textContent = "端口: ";

    const portValueSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
    portValueSpan.textContent = portInfo;

    portText.appendChild(portTitleSpan);
    portText.appendChild(portValueSpan);

    // 管理人
    const managerText = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    managerText.setAttribute("x", x + width / 2);
    managerText.setAttribute("y", y + 130);

    const managerTitleSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
    managerTitleSpan.textContent = "管理人: ";

    const managerValueSpan = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "tspan",
    );
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

    // 组装元素
    group.appendChild(rect);
    group.appendChild(nameText);
    group.appendChild(ipText);
    group.appendChild(portText);
    group.appendChild(managerText);
    this.elementsGroup.appendChild(group);

    return group;
  }

  // 绘制机柜
  drawCabinet(cabinet) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-element";
    group.dataset.id = cabinet.id;

    const x = cabinet.position.x || 50;
    const y = cabinet.position.y || 50;
    const width = cabinet.position.width || 150; // 增加机柜宽度
    const capacity = cabinet.capacity || 45; // 默认45U
    const height = cabinet.position.height || (capacity * 20 + 40); // 根据capacity计算高度，包含标题区域

    // 机柜矩形
    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", width);
    rect.setAttribute("height", height);

    // 机柜图标（服务器图标）
    const iconGroup = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "g",
    );
    iconGroup.className.baseVal = "cabinet-icon";

    // 绘制简单的服务器图标
    const iconX = x + width - 25;
    const iconY = y + 5;

    // 服务器机箱
    const serverRect = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "rect",
    );
    serverRect.setAttribute("x", iconX);
    serverRect.setAttribute("y", iconY);
    serverRect.setAttribute("width", 18);
    serverRect.setAttribute("height", 12);
    serverRect.setAttribute("rx", 2);
    serverRect.setAttribute("fill", "#4a90d9");
    serverRect.setAttribute("stroke", "#2c5282");
    serverRect.setAttribute("stroke-width", "1");

    // 指示灯
    const led1 = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "circle",
    );
    led1.setAttribute("cx", iconX + 4);
    led1.setAttribute("cy", iconY + 6);
    led1.setAttribute("r", 2);
    led1.setAttribute("fill", "#48bb78");

    const led2 = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "circle",
    );
    led2.setAttribute("cx", iconX + 10);
    led2.setAttribute("cy", iconY + 6);
    led2.setAttribute("r", 2);
    led2.setAttribute("fill", "#f6ad55");

    iconGroup.appendChild(serverRect);
    iconGroup.appendChild(led1);
    iconGroup.appendChild(led2);

    // 机柜名称
    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", x + width / 2);
    text.setAttribute("y", y + 20);
    text.textContent = cabinet.name;
    group.dataset.tooltip = `机柜: ${cabinet.name}\n容量: ${cabinet.capacity || 42}U`;

    // 绘制U位标记
    this.drawUMarks(cabinet, group);

    // 组装元素
    group.appendChild(rect);
    group.appendChild(iconGroup);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  // 绘制U位标记
  drawUMarks(cabinet, group) {
    const uHeight = 20;
    const startY = (cabinet.position.y || 50) + 40;
    const width = cabinet.position.width || 150; // 适应新的机柜宽度
    const capacity = cabinet.capacity || 45; // 使用capacity字段，默认45U

    for (let i = 1; i <= capacity; i++) {
      const y = startY + (capacity - i) * uHeight;

      // U位线
      const line = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "line",
      );
      line.setAttribute("x1", (cabinet.position.x || 50) + 10);
      line.setAttribute("y1", y);
      line.setAttribute("x2", (cabinet.position.x || 50) + width - 10);
      line.setAttribute("y2", y);
      line.setAttribute("stroke", "#e0e0e0");
      line.setAttribute("stroke-width", "0.5");
      group.appendChild(line);

      // U位标记（每5U显示一次数字，避免过于密集）
      if (i % 5 === 0 || i === 1 || i === capacity) {
        const text = document.createElementNS(
          "http://www.w3.org/2000/svg",
          "text",
        );
        text.setAttribute("x", (cabinet.position.x || 50) + 5);
        text.setAttribute("y", y + uHeight / 2);
        text.textContent = i;
        text.className.baseVal = "u-mark";
        group.appendChild(text);
      }
    }
  }

  // 绘制机位
  drawCabinetPosition(position, cabinet) {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "cabinet-position-element";
    group.dataset.id = position.id;

    // 计算机位位置和大小
    const uHeight = 20;
    const startU = position.start_u || 1;
    const endU = position.end_u || startU;
    const height = Math.max((endU - startU + 1) * uHeight, uHeight);

    // 机柜参数
    const cabinetY = cabinet.position?.y || 50;
    const cabinetX = cabinet.position?.x || 50;
    const capacity = cabinet.capacity || 45; // 默认45U

    // 从底部开始计算位置（U位从底部向上编号）
    // startY 是机柜内容区域的起始位置（跳过标题区域40px）
    const startY = cabinetY + 40;
    // 计算机位的Y坐标：从底部向上，第1U在最底部
    const y = startY + (capacity - endU) * uHeight;

    // 机位矩形
    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", cabinetX + 20); // 适应新的机柜宽度
    rect.setAttribute("y", y);
    rect.setAttribute("width", 110); // 增加机位宽度
    rect.setAttribute("height", height);

    // 机位名称
    const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
    text.setAttribute("x", cabinetX + 75); // 调整文本位置
    text.setAttribute("y", y + height / 2 + 4);
    text.textContent = position.name;

    // 端口信息
    const portsLabel =
      position.ports && position.ports.length > 0
        ? position.ports
            .map((port) => `${port.network_name || "未知网络"}: ${port.port_number || "未知端口"}`)
            .join(", ")
        : "无端口";
    
    // IP信息
    const ipAddress = position.ipManager ? position.ipManager.ip_address : "无IP";
    
    group.dataset.tooltip = `机位: ${position.name}\nU位: ${startU}-${endU}\nIP: ${ipAddress}\n${portsLabel}`;

    // 组装元素
    group.appendChild(rect);
    group.appendChild(text);
    this.elementsGroup.appendChild(group);

    return group;
  }

  // 绘制门图标
  drawDoor() {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.className.baseVal = "door-element";
    // 使用固定的UUID作为门的ID，确保每次绘制时ID保持一致
    group.dataset.id = "00000000-0000-0000-0000-000000000001";
    
    // 门的位置（在工位区域外作为参考点）
    const x = 50;
    const y = 100;
    const width = 40;
    const height = 80;
    
    // 门的外框
    const doorFrame = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    doorFrame.setAttribute("x", x);
    doorFrame.setAttribute("y", y);
    doorFrame.setAttribute("width", width);
    doorFrame.setAttribute("height", height);
    doorFrame.setAttribute("fill", "#d1d5db");
    doorFrame.setAttribute("stroke", "#6b7280");
    doorFrame.setAttribute("stroke-width", "2");
    
    // 门把手
    const doorHandle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    doorHandle.setAttribute("cx", x + width - 10);
    doorHandle.setAttribute("cy", y + height / 2);
    doorHandle.setAttribute("r", 3);
    doorHandle.setAttribute("fill", "#6b7280");
    
    // 门标签
    const doorLabel = document.createElementNS("http://www.w3.org/2000/svg", "text");
    doorLabel.setAttribute("x", x + width / 2);
    doorLabel.setAttribute("y", y - 10);
    doorLabel.textContent = "门";
    
    // 添加提示信息
    group.dataset.tooltip = "房间入口参考点";
    
    // 组装元素
    group.appendChild(doorFrame);
    group.appendChild(doorHandle);
    group.appendChild(doorLabel);
    this.elementsGroup.appendChild(group);
    
    return group;
  }

  // 自动绘制工位
  async autoDrawWorkstations(roomId) {
    try {
      // 尝试加载保存的布局
      const hasSavedLayout = await this.loadSavedLayout(roomId);
      
      // 从后端获取工位数据
      const workstations = await this.fetchWorkstationsByRoom(roomId);
      const ipManagers = await this.fetchIpManager();
      const ipMap = new Map();
      if (Array.isArray(ipManagers)) {
        ipManagers.forEach((ipManager) => {
          if (!ipManager.workstation_id) return;
          const existing = ipMap.get(ipManager.workstation_id);
          if (!existing ||
              (existing.status !== "active" && ipManager.status === "active")) {
            ipMap.set(ipManager.workstation_id, ipManager);
          }
        });
      }

      // 如果已存在工位图数据，直接加载并显示现有工位图，无需重新绘制
      if (hasSavedLayout) {
        this.currentRoomId = roomId;
        return;
      }

      this.currentRoomId = roomId;

      // 如果不存在工位图数据，清空现有元素
      this.elementsGroup.innerHTML = "";

      // 如果没有工位数据，不显示任何内容
      if (workstations.length === 0) {
        console.log("该房间下暂无工位数据");
        showMessage("该房间下暂无工位数据", "info");
        return;
      }

      // 绘制门图标作为参考点
      this.drawDoor();

      // 计算布局
      const gap = 20;
      const width = 160; // 大小翻倍
      const height = 160; // 大小翻倍
      const startX = 150; // 调整起始位置，为门图标留出空间
      const startY = 100;
      
      // 根据工位数量动态调整列数
      let cols = Math.min(8, Math.max(3, Math.ceil(Math.sqrt(workstations.length))));
      if (workstations.length <= 5) {
        cols = 3; // 少于等于5个工位时使用3列
      } else if (workstations.length <= 12) {
        cols = 4; // 少于等于12个工位时使用4列
      } else if (workstations.length <= 24) {
        cols = 6; // 少于等于24个工位时使用6列
      }
      
      const rows = Math.ceil(workstations.length / cols);

      // 绘制工位
      workstations.forEach((workstation, index) => {
        const col = index % cols;
        const row = Math.floor(index / cols);

        workstation.ipManager = ipMap.get(workstation.id) || null;
        workstation.position = {
          x: startX + col * (width + gap),
          y: startY + row * (height + gap),
          width: width,
          height: height,
        };

        this.drawWorkstation(workstation);
      });

      // 根据实际绘制的内容调整viewBox
      const totalWidth = startX + cols * (width + gap) + 50;
      const totalHeight = startY + rows * (height + gap) + 50;
      this.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, totalWidth)} ${Math.max(800, totalHeight)}`);
      
      // 自动保存布局到数据库
      setTimeout(() => {
        this.saveLayout();
      }, 500);
    } catch (error) {
      console.error("自动绘制工位失败:", error);
      showToast("自动绘制工位失败，请重试", "error");
    }
  }

  // 加载保存的布局
  async loadSavedLayout(id) {
    try {
      if (this.type === "workstation") {
        // 设置当前房间ID
        this.currentRoomId = id;
        
        // 清空现有元素，避免重叠
        this.elementsGroup.innerHTML = "";
        
        // 并行获取数据，提高加载速度
        const [layoutResponse, workstations, ipManagers] = await Promise.all([
          fetch(`/api/resources/layouts/workstation/${id}`, {
            method: "GET",
            headers: {
              "Authorization": `Bearer ${getAccessToken()}`,
            },
          }),
          this.fetchWorkstationsByRoom(id),
          this.fetchIpManager()
        ]);
        
        // 处理IP数据
        const ipMap = new Map();
        if (Array.isArray(ipManagers)) {
          ipManagers.forEach((ipManager) => {
            if (!ipManager.workstation_id) return;
            ipMap.set(ipManager.workstation_id, ipManager);
          });
        }
        
        // 处理布局数据
        let layoutData = [];
        let hasSavedLayout = false;
        
        if (layoutResponse.ok) {
          const result = await layoutResponse.json();
          if (result.success && result.data && Array.isArray(result.data) && result.data.length > 0) {
            layoutData = result.data;
            hasSavedLayout = true;
            console.log("从数据库加载布局数据成功");
          } else {
            console.log("数据库中没有布局数据");
          }
        } else {
          console.warn("从API加载布局数据失败");
          // API加载失败时，只绘制门图标，不绘制工位
          console.log("API加载失败，不绘制工位");
        }
        
        // 绘制门图标作为参考点
        const doorElement = this.drawDoor();
        
        // 记录最大的x和y坐标，用于调整viewBox
        let maxX = 0;
        let maxY = 0;
        
        // 恢复门的位置
        const doorItem = layoutData.find(item => item.element_type === "door" || item.id === "00000000-0000-0000-0000-000000000001");
        if (doorItem && doorItem.position) {
          const rect = doorElement.querySelector("rect");
          if (rect) {
            rect.setAttribute("x", doorItem.position.x);
            rect.setAttribute("y", doorItem.position.y);
            rect.setAttribute("width", doorItem.position.width);
            rect.setAttribute("height", doorItem.position.height);
          }
          
          // 更新门的其他元素位置
          const text = doorElement.querySelector("text");
          if (text) {
            text.setAttribute("x", doorItem.position.x + doorItem.position.width / 2);
            text.setAttribute("y", doorItem.position.y - 10);
          }
          
          const circle = doorElement.querySelector("circle");
          if (circle) {
            circle.setAttribute("cx", doorItem.position.x + doorItem.position.width - 10);
            circle.setAttribute("cy", doorItem.position.y + doorItem.position.height / 2);
          }
          
          // 更新最大坐标
          maxX = Math.max(maxX, doorItem.position.x + doorItem.position.width);
          maxY = Math.max(maxY, doorItem.position.y + doorItem.position.height);
        } else {
          // 如果没有保存的门位置，使用默认位置
          const rect = doorElement.querySelector("rect");
          if (rect) {
            const x = parseFloat(rect.getAttribute("x"));
            const y = parseFloat(rect.getAttribute("y"));
            const width = parseFloat(rect.getAttribute("width"));
            const height = parseFloat(rect.getAttribute("height"));
            maxX = Math.max(maxX, x + width);
            maxY = Math.max(maxY, y + height);
          }
        }
        
        // 绘制工位
        if (hasSavedLayout) {
          workstations.forEach((workstation, index) => {
            // 查找保存的布局数据，忽略大小写差异
            const savedItem = layoutData.find(item => item.id.toLowerCase() === workstation.id.toLowerCase());
            if (savedItem && savedItem.position) {
              workstation.position = savedItem.position;
            } else {
              // 如果没有保存的布局数据，生成不同的默认位置，避免重叠
              const gap = 20;
              const width = 160;
              const height = 160;
              const cols = 4;
              const col = index % cols;
              const row = Math.floor(index / cols);
              workstation.position = {
                x: 150 + col * (width + gap),
                y: 100 + row * (height + gap),
                width: width,
                height: height
              };
            }
            workstation.ipManager = ipMap.get(workstation.id);
            this.drawWorkstation(workstation);
            
            // 更新最大坐标
            const pos = workstation.position;
            maxX = Math.max(maxX, pos.x + pos.width);
            maxY = Math.max(maxY, pos.y + pos.height);
          });
        } else {
          // 如果没有保存的布局数据，只绘制门图标，不绘制工位
          console.log("没有保存的布局数据，不绘制工位");
        }
        
        // 根据最大坐标调整viewBox
        if (maxX > 0 || maxY > 0) {
          const padding = 50;
          this.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, maxX + padding)} ${Math.max(800, maxY + padding)}`);
        }
        
        return hasSavedLayout; // 成功加载现有布局
      } else if (this.type === "cabinet") {
        // 设置当前网络区域ID
        this.currentNetworkRegionId = id;
        
        // 清空现有元素，避免重叠
        this.elementsGroup.innerHTML = "";
        
        // 并行获取数据，提高加载速度
        const [layoutResponse, cabinetsResult] = await Promise.all([
          fetch(`/api/resources/layouts/positions/${id}`, {
            method: "GET",
            headers: {
              "Authorization": `Bearer ${getAccessToken()}`,
            },
          }),
          fetch(`/api/resources/cabinets`, {
            headers: {
              "Authorization": `Bearer ${getAccessToken()}`,
            },
          })
        ]);
        
        // 处理布局数据
        let layoutData = [];
        let hasSavedLayout = false;
        
        if (layoutResponse.ok) {
          const result = await layoutResponse.json();
          if (result.success && result.data && Array.isArray(result.data) && result.data.length > 0) {
            layoutData = result.data;
            hasSavedLayout = true;
            console.log("从数据库加载网络区域布局数据成功");
          } else {
            console.log("数据库中没有网络区域布局数据");
            showMessage("数据库中没有网络区域布局数据", "info");
          }
        } else {
          console.warn("从API加载网络区域布局数据失败");
          showMessage("加载布局数据失败", "error");
        }
        
        // 处理机柜数据
        let filteredCabinets = [];
        if (cabinetsResult.ok) {
          const cabinetsData = await cabinetsResult.json();
          if (cabinetsData.success && cabinetsData.data) {
            // 过滤出指定网络区域的机柜
            filteredCabinets = cabinetsData.data.filter(cabinet => {
              return cabinet.networks && cabinet.networks.some(network => 
                network.network_region_id === id
              );
            });
          }
        }
        
        // 绘制机柜
        if (hasSavedLayout && filteredCabinets.length > 0) {
          // 记录最大的x和y坐标，用于调整viewBox
          let maxX = 0;
          let maxY = 0;
          
          // 绘制过滤后的机柜
          filteredCabinets.forEach((cabinet, index) => {
            // 查找保存的布局数据，忽略大小写差异
            const savedItem = layoutData.find(item => item.id.toLowerCase() === cabinet.id.toLowerCase());
            if (savedItem && savedItem.position) {
              // 应用保存的位置信息
              cabinet.position = savedItem.position;
            } else {
              // 如果没有保存的位置，使用默认位置
              const cabinetWidth = 150;
              const gap = 50;
              const startX = 50;
              const bottomY = 600;
              cabinet.capacity = cabinet.capacity || 45;
              const uHeight = 20;
              const cabinetHeight = cabinet.capacity * uHeight + 40;
              
              cabinet.position = {
                x: startX + index * (cabinetWidth + gap),
                y: bottomY - cabinetHeight,
                width: cabinetWidth,
                height: cabinetHeight,
              };
            }
            
            // 绘制机柜
            this.drawCabinet(cabinet);
            
            // 绘制机位
            this.drawCabinetPositions(cabinet);
            
            // 更新最大坐标
            if (cabinet.position) {
              maxX = Math.max(maxX, cabinet.position.x + cabinet.position.width);
              maxY = Math.max(maxY, cabinet.position.y + cabinet.position.height);
            }
          });
          
          // 调整SVG视图，确保所有元素可见
          if (maxX > 0 || maxY > 0) {
            const padding = 50;
            this.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, maxX + padding)} ${Math.max(600, maxY + padding)}`);
          }
        } else {
          // 没有保存的布局，不显示任何元素
          console.log("没有保存的网络区域布局数据");
        }
        
        return hasSavedLayout;
      }
    } catch (error) {
      console.error("加载布局失败:", error);
      showToast("加载布局失败", "error");
      // 发生错误时清空显示
      this.elementsGroup.innerHTML = "";
      return false;
    }
  }



  // 自动绘制机位图
  async autoDrawCabinetPositions(networkRegionId) {
    if (!networkRegionId) {
      console.warn("未选择网络区域");
      return;
    }

    try {
      // 从后端获取指定网络区域的机柜数据
      // 注意：这里假设API支持根据网络区域ID过滤机柜
      // 如果不支持，需要修改API或在前端过滤
      const cabinetsResult = await apiGet("/api/resources/cabinets");
      if (!cabinetsResult.success || !cabinetsResult.data || cabinetsResult.data.length === 0) {
        console.error("获取机柜数据失败");
        showMessage("获取机柜数据失败", "error");
        return;
      }

      // 过滤出指定网络区域的机柜
      // 根据机柜的networks数组中的network_region_id字段进行过滤
      const filteredCabinets = cabinetsResult.data.filter(cabinet => {
        // 检查机柜是否关联了指定网络区域的网络
        return cabinet.networks && cabinet.networks.some(network => 
          network.network_region_id === networkRegionId
        );
      });
      
      if (filteredCabinets.length === 0) {
        console.log("该网络区域下暂无机柜数据");
        showMessage("该网络区域下暂无机柜数据", "info");
        return;
      }

      // 清空现有元素
      this.elementsGroup.innerHTML = "";

      // 如果没有机柜数据，不显示任何内容
      if (filteredCabinets.length === 0) {
        console.log("该网络区域下暂无机柜数据");
        showMessage("该网络区域下暂无机柜数据", "info");
        return;
      }

      // 计算布局 - 多个机柜横向排列
      const cabinetWidth = 150; // 增加机柜宽度
      const gap = 50; // 机柜间距
      const startX = 50;
      
      // 获取容器的实际高度，实现自适应布局
      const containerHeight = this.container.clientHeight || 800; // 容器高度，默认800px
      
      // 计算SVG可视区域高度，基于容器高度
      const topPadding = 20; // 顶部预留空间
      const bottomPadding = 20; // 底部预留空间
      const availableHeight = containerHeight - topPadding - bottomPadding;
      
      // 45U机柜标准高度：45 * 20 + 40 = 940px
      // 计算合适的SVG高度，确保45U机柜有足够空间
      const svgHeight = Math.max(availableHeight + topPadding + bottomPadding, 1000);
      const bottomY = svgHeight - bottomPadding; // 底部对齐的基准线

      // 绘制过滤后的机柜
      filteredCabinets.forEach((cabinet, index) => {
        // 计算机位
        const x = startX + index * (cabinetWidth + gap);
        
        // 设置默认值
        cabinet.capacity = cabinet.capacity || 45; // 默认45U
        const uHeight = 20; // 每U高度像素
        const cabinetHeight = cabinet.capacity * uHeight + 40; // 包含标题区域

        // 计算机柜的Y坐标，使所有机柜底部对齐
        // 从底部基准线向上计算，减去机柜高度
        const y = bottomY - cabinetHeight;

        // 设置机位参数
        cabinet.position = {
          x: x,
          y: y,
          width: cabinetWidth,
          height: cabinetHeight,
        };

        // 绘制机柜
        this.drawCabinet(cabinet);

        // 绘制机位
        this.drawCabinetPositions(cabinet);
      });

      // 调整SVG viewBox，确保所有机柜可见
      const totalWidth = startX + filteredCabinets.length * (cabinetWidth + gap) + 50;
      this.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, totalWidth)} ${svgHeight}`);
      
      // 调整SVG元素高度，匹配计算出的高度
      this.svg.setAttribute("height", svgHeight);

      this.currentNetworkRegionId = networkRegionId;
    } catch (error) {
      console.error("自动绘制机位图失败:", error);
      showMessage("绘制机位图失败: " + error.message, "error");
    }
  }

  // 绘制机柜的机位
  async drawCabinetPositions(cabinet) {
    try {
      // 从后端获取机位数据
      const positionsResult = await apiGet(`/api/resources/positions?cabinet_id=${cabinet.id}`);
      if (!positionsResult.success || !positionsResult.data) {
        console.log("该机柜暂无机位数据:", cabinet.name);
        return;
      }

      const positions = positionsResult.data;

      // 获取IP数据
      const ipResult = await apiGet("/api/resources/ip");
      const ipMap = new Map();
      
      if (ipResult.success && ipResult.data) {
        ipResult.data.forEach(ipManager => {
          if (ipManager.cabinet_position_id) {
            ipMap.set(ipManager.cabinet_position_id, ipManager);
          }
        });
      }

      // 绘制机位
      if (positions && positions.length > 0) {
        positions.forEach((position) => {
          // 关联IP数据
          position.ipManager = ipMap.get(position.id) || null;
          this.drawCabinetPosition(position, cabinet);
        });
      } else {
        console.log("该机柜暂无机位数据:", cabinet.name);
      }
    } catch (error) {
      console.error("绘制机位失败:", error);
    }
  }

  // 从后端获取工位数据
  async fetchWorkstationsByRoom(roomId) {
    try {
      const result = await apiGet(
        `/api/resources/workstations?room_id=${roomId}`,
      );
      return result.success ? result.data : [];
    } catch (error) {
      console.error("获取工位数据失败:", error);
      return [];
    }
  }

  // 从后端获取机柜数据
  async fetchCabinetById(cabinetId) {
    try {
      const result = await apiGet(`/api/resources/cabinets/${cabinetId}`);
      return result.success ? result.data : null;
    } catch (error) {
      console.error("获取机柜数据失败:", error);
      return null;
    }
  }

  // 从后端获取机位数据
  async fetchCabinetPositionsByCabinet(cabinetId) {
    try {
      const result = await apiGet(
        `/api/resources/positions?cabinet_id=${cabinetId}`,
      );
      return result.success ? result.data : [];
    } catch (error) {
      console.error("获取机位数据失败:", error);
      return [];
    }
  }

  async fetchIpManager() {
    try {
      const result = await apiGet("/api/resources/ip");
      if (result.success && result.data) {
        if (Array.isArray(result.data)) {
          return result.data;
        }
        if (result.data.data && Array.isArray(result.data.data)) {
          return result.data.data;
        }
        if (result.data.ip_managers && Array.isArray(result.data.ip_managers)) {
          return result.data.ip_managers;
        }
        if (result.data.ips && Array.isArray(result.data.ips)) {
          return result.data.ips;
        }
      }
      return [];
    } catch (error) {
      console.error("获取IP失败:", error);
      return [];
    }
  }

  // 保存布局
  async saveLayout() {
    // 收集元素位置信息
    const elements = this.elementsGroup.querySelectorAll("[data-id]");
    const layoutData = [];

    elements.forEach((el) => {
        const id = el.dataset.id;
        const rect = el.querySelector("rect");
        if (!rect) return;

        const position = {
          x: parseFloat(rect.getAttribute("x")),
          y: parseFloat(rect.getAttribute("y")),
          width: parseFloat(rect.getAttribute("width")),
          height: parseFloat(rect.getAttribute("height")),
          rotation: 0, // 添加rotation字段，默认为0
        };

        // 确定元素类型
        let element_type;
        if (this.type === "workstation") {
          element_type = el.classList.contains("door-element") ? "door" : "workstation";
        } else if (this.type === "cabinet") {
          element_type = "network_device";
        } else {
          element_type = "workstation";
        }

        layoutData.push({ id, position, element_type });
    });

    // 发送到后端保存
    try {
      const response = await fetch("/api/resources/layouts", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${getAccessToken()}`,
        },
        body: JSON.stringify({
          type: this.type === "cabinet" ? "network_region" : this.type,
          room_id: this.type === "workstation" ? this.currentRoomId : null,
          network_region_id: this.type === "cabinet" ? this.currentNetworkRegionId : null,
          cabinet_id: null,
          layout: layoutData,
        }),
      });

      const result = await response.json();
      if (result.success) {
        showToast("布局保存成功", "success");
      } else {
        showToast("布局保存失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("保存布局失败:", error);
      showToast("布局保存失败", "error");
    }
  }

  // 删除布局
  async deleteLayout() {
    if (this.type === "workstation") {
      if (!this.currentRoomId) {
        showToast("请先选择房间", "warning");
        return;
      }

      if (!confirm("确定要删除当前房间的工位布局数据吗？此操作不可恢复。")) {
        return;
      }

      try {
        // 发送删除请求到后端
        const response = await fetch(`/api/resources/layouts/workstation/${this.currentRoomId}`, {
          method: "DELETE",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${getAccessToken()}`,
          },
        });

        const result = await response.json();
        if (result.success) {
          // 清空显示
          this.elementsGroup.innerHTML = "";
          showToast("布局删除成功", "success");
        } else {
          showToast("布局删除失败: " + result.message, "error");
        }
      } catch (error) {
        console.error("删除布局失败:", error);
        // 即使API调用失败，也清空显示
        this.elementsGroup.innerHTML = "";
        showToast("布局删除成功", "success");
      }
    } else if (this.type === "cabinet") {
      if (!this.currentNetworkRegionId) {
        showToast("请先选择网络区域", "warning");
        return;
      }

      if (!confirm("确定要删除当前网络区域的机位布局数据吗？此操作不可恢复。")) {
        return;
      }

      try {
        // 发送删除请求到后端
        const response = await fetch(`/api/resources/layouts/positions/${this.currentNetworkRegionId}`, {
          method: "DELETE",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${getAccessToken()}`,
          },
        });

        const result = await response.json();
        if (result.success) {
          // 清空显示
          this.elementsGroup.innerHTML = "";
          showToast("布局删除成功", "success");
        } else {
          showToast("布局删除失败: " + result.message, "error");
        }
      } catch (error) {
        console.error("删除布局失败:", error);
        // 即使API调用失败，也清空显示
        this.elementsGroup.innerHTML = "";
        showToast("布局删除成功", "success");
      }
    }
  }

  // 删除工位
  deleteWorkstation(id) {
    const workstation = this.elementsGroup.querySelector(`[data-id="${id}"]`);
    if (workstation) {
      workstation.remove();
      showToast("工位删除成功", "success");
      this.saveLayout();
    }
  }

  // 删除机位
  deleteCabinetPosition(id) {
    const cabinetPosition = this.elementsGroup.querySelector(`[data-id="${id}"]`);
    if (cabinetPosition) {
      cabinetPosition.remove();
      showToast("机位删除成功", "success");
      this.saveLayout();
    }
  }
}
