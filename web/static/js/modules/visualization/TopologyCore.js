import { SVG_NS } from "./SVGCore.js";
import { showToast } from "../../utils/ui.js";
import { t } from "../../utils/i18n.js";

export class TopologyCore {
  constructor(containerId, callbacks = {}) {
    this.container = document.getElementById(containerId);
    this.svg = null;
    this.connectionsGroup = null;
    this.elementsGroup = null;
    this.tempConnectionGroup = null;
    this.selectedElement = null;
    this.isDragging = false;
    this.hasMoved = false;
    this.elementStartPos = { x: 0, y: 0 };
    this.mouseStartPos = { x: 0, y: 0 };
    this.isPanning = false;
    this.panStart = { x: 0, y: 0 };
    this.viewBoxStart = { x: 0, y: 0 };
    // 容器（房间/机柜分组框）整体拖动状态
    this.isContainerDragging = false;
    this.containerDrag = null;
    this.isSpaceDown = false;
    this.isConnectionMode = false;
    this.connectionSource = null;
    this.gridSize = 20;
    this.callbacks = callbacks;

    this._init();
  }

  _init() {
    this._initSVG();
    this._initTooltip();
    this._initEventListeners();
  }

  _initTooltip() {
    const tooltipId = "visualization-tooltip-topology";
    const existing = document.getElementById(tooltipId);
    if (existing) {
      this.tooltip = existing;
      return;
    }
    const tooltip = document.createElement("div");
    tooltip.id = tooltipId;
    tooltip.className = "tooltip";
    document.body.appendChild(tooltip);
    this.tooltip = tooltip;
  }

  _initSVG() {
    this.container.innerHTML = "";

    this.svg = document.createElementNS(SVG_NS, "svg");
    this.svg.className.baseVal = "visualization-svg";
    this.svg.setAttribute("width", "100%");
    this.svg.setAttribute("height", "100%");
    this.svg.setAttribute("viewBox", "0 0 3000 2000");
    // 内容锚定左上角：默认 xMidYMid 会在容器宽高比与 viewBox 不一致时把网格
    // 整体居中，画布顶缘留出空白带，视觉上画布与工具栏之间偏离一段距离
    this.svg.setAttribute("preserveAspectRatio", "xMinYMin meet");

    this._createDefs();

    const bg = document.createElementNS(SVG_NS, "rect");
    bg.setAttribute("x", 0);
    bg.setAttribute("y", 0);
    bg.setAttribute("width", 3000);
    bg.setAttribute("height", 2000);
    bg.setAttribute("fill", "url(#topology-grid)");
    this.svg.appendChild(bg);
    // 背景矩形需随 viewBox 显式更新：百分比按容器像素而非 viewBox 解析，
    // 缩小视野（fitView/缩小）时会导致网格只覆盖画布局部
    this.gridRect = bg;

    // 主网格线 + 坐标标尺层（元素层之下，viewBox 变化时重绘）
    this.gridRulerGroup = document.createElementNS(SVG_NS, "g");
    this.gridRulerGroup.className.baseVal = "grid-ruler";
    this.svg.appendChild(this.gridRulerGroup);
    this._renderGridRuler();

    this.containersGroup = document.createElementNS(SVG_NS, "g");
    this.containersGroup.className.baseVal = "containers-group";
    this.svg.appendChild(this.containersGroup);

    this.connectionsGroup = document.createElementNS(SVG_NS, "g");
    this.connectionsGroup.className.baseVal = "connections-group";
    this.svg.appendChild(this.connectionsGroup);

    this.elementsGroup = document.createElementNS(SVG_NS, "g");
    this.elementsGroup.className.baseVal = "elements-group";
    this.svg.appendChild(this.elementsGroup);

    this.tempConnectionGroup = document.createElementNS(SVG_NS, "g");
    this.tempConnectionGroup.className.baseVal = "temp-connection-group";
    this.svg.appendChild(this.tempConnectionGroup);

    this.container.appendChild(this.svg);

    // 尺寸/边框/背景由 CSS（visualization.css 的 #global-visualization-container，
    // flex:1 随可用空间伸缩）控制：内联固定高度 calc(100vh - 200px) 会覆盖 flex
    // 布局，工具栏实际占高与 200px 不符时画布顶缘错位、底部溢出
    this.container.style.overflow = "hidden";
  }

  _createDefs() {
    const defs = document.createElementNS(SVG_NS, "defs");
    const pattern = document.createElementNS(SVG_NS, "pattern");
    pattern.setAttribute("id", "topology-grid");
    pattern.setAttribute("width", this.gridSize);
    pattern.setAttribute("height", this.gridSize);
    pattern.setAttribute("patternUnits", "userSpaceOnUse");
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", `M ${this.gridSize} 0 L 0 0 0 ${this.gridSize}`);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "#e5e7eb");
    path.setAttribute("stroke-width", "0.5");
    pattern.appendChild(path);
    defs.appendChild(pattern);
    this.svg.appendChild(defs);
  }

  /**
   * 按当前 viewBox 重绘主网格与坐标标注。
   * 主网格间距为 gridSize 的 5 倍（100px），顶边/左边标注画布坐标，
   * 配合设备/容器坐标输入框精确定位。
   */
  _renderGridRuler() {
    if (!this.gridRulerGroup) {
      return;
    }
    const vb = this.svg.viewBox.baseVal;
    const group = this.gridRulerGroup;
    group.innerHTML = "";
    if (!vb.width || !vb.height) {
      return;
    }

    const step = this.gridSize * 5;
    // 标注字号随视口宽度缩放，缩放后保持可读
    const fontSize = Math.max(9, Math.min(14, vb.width / 150));
    const endX = vb.x + vb.width;
    const endY = vb.y + vb.height;

    for (let gx = Math.floor(vb.x / step) * step; gx <= endX; gx += step) {
      const line = document.createElementNS(SVG_NS, "line");
      line.setAttribute("x1", gx);
      line.setAttribute("y1", vb.y);
      line.setAttribute("x2", gx);
      line.setAttribute("y2", endY);
      line.setAttribute("class", "grid-major-line");
      group.appendChild(line);

      const label = document.createElementNS(SVG_NS, "text");
      label.setAttribute("x", gx + 2);
      label.setAttribute("y", vb.y + fontSize);
      label.setAttribute("class", "grid-label");
      label.setAttribute("font-size", fontSize);
      label.textContent = gx;
      group.appendChild(label);
    }

    for (let gy = Math.floor(vb.y / step) * step; gy <= endY; gy += step) {
      const line = document.createElementNS(SVG_NS, "line");
      line.setAttribute("x1", vb.x);
      line.setAttribute("y1", gy);
      line.setAttribute("x2", endX);
      line.setAttribute("y2", gy);
      line.setAttribute("class", "grid-major-line");
      group.appendChild(line);

      const label = document.createElementNS(SVG_NS, "text");
      label.setAttribute("x", vb.x + 2);
      label.setAttribute("y", gy - 2);
      label.setAttribute("class", "grid-label");
      label.setAttribute("font-size", fontSize);
      label.textContent = gy;
      group.appendChild(label);
    }
  }

  /** 统一的 viewBox 更新入口：背景矩形同步铺满，rAF 节流重绘主网格与坐标标注（平移/缩放高频触发）。 */
  setViewBox(x, y, width, height) {
    this.svg.setAttribute("viewBox", `${x} ${y} ${width} ${height}`);
    if (this.gridRect) {
      this.gridRect.setAttribute("x", x);
      this.gridRect.setAttribute("y", y);
      this.gridRect.setAttribute("width", width);
      this.gridRect.setAttribute("height", height);
    }
    if (this._gridRulerRaf) {
      return;
    }
    this._gridRulerRaf = requestAnimationFrame(() => {
      this._gridRulerRaf = 0;
      this._renderGridRuler();
    });
  }

  _initEventListeners() {
    this.svg.addEventListener("mousedown", this._handleMouseDown.bind(this));
    this.svg.addEventListener("mousemove", this._handleMouseMove.bind(this));
    this.svg.addEventListener("mouseup", this._handleMouseUp.bind(this));
    this.svg.addEventListener("wheel", this._handleWheel.bind(this), { passive: false });
    this.svg.addEventListener("mouseleave", () => {
      this._handleMouseUp();
      this._hideTooltip();
      this.tempConnectionGroup.innerHTML = "";
    });

    document.addEventListener("keydown", (e) => {
      if (e.code === "Space" && !e.target.closest("input, select, textarea")) {
        e.preventDefault();
        this.isSpaceDown = true;
        this.container.classList.add("panning");
      }
      if (e.code === "Escape") {
        this._cancelConnection();
      }
    });

    document.addEventListener("keyup", (e) => {
      if (e.code === "Space") {
        this.isSpaceDown = false;
        this.container.classList.remove("panning");
      }
    });
  }

  _handleMouseDown(e) {
    if (this.isConnectionMode) {
      const anchor = e.target.closest(".port-anchor");
      if (anchor) {
        this._handleAnchorClick(anchor, e);
      }
      return;
    }

    if (e.button === 1 || (e.button === 0 && this.isSpaceDown)) {
      e.preventDefault();
      this.isPanning = true;
      this.panStart = { x: e.clientX, y: e.clientY };
      const vb = this.svg.viewBox.baseVal;
      this.viewBoxStart = { x: vb.x, y: vb.y };
      return;
    }

    if (e.button !== 0) {
      return;
    }

    const anchor = e.target.closest(".port-anchor");
    if (anchor) {
      return;
    }

    // 容器（房间/机柜分组框）拖动：整体移动组内全部设备节点
    const containerG = e.target.closest(".topology-container-group");
    if (containerG && this._startContainerDrag(containerG, e)) {
      return;
    }

    const target = e.target.closest("[data-device-id]");
    if (target) {
      this.isDragging = true;
      this.hasMoved = false;
      this.selectedElement = target;
      this._selectElement(target);

      const rect = target.querySelector("rect");
      if (rect) {
        this.elementStartPos = {
          x: parseFloat(rect.getAttribute("x")),
          y: parseFloat(rect.getAttribute("y"))
        };
      }
      this.mouseStartPos = this._getSvgCoordinates(e);
    } else {
      // 空白处左键拖拽平移画布（中键/空格+拖拽保留）
      this._clearSelection();
      if (this.callbacks.onCanvasClick) {
        this.callbacks.onCanvasClick();
      }
      this.isPanning = true;
      this.panStart = { x: e.clientX, y: e.clientY };
      const vb = this.svg.viewBox.baseVal;
      this.viewBoxStart = { x: vb.x, y: vb.y };
      this.container.classList.add("panning");
    }
  }

  _handleMouseMove(e) {
    if (this.isPanning) {
      this._hideTooltip();
      const vb = this.svg.viewBox.baseVal;
      const scale = vb.width / this.container.clientWidth;
      const dx = (e.clientX - this.panStart.x) * scale;
      const dy = (e.clientY - this.panStart.y) * scale;
      this.setViewBox(this.viewBoxStart.x - dx, this.viewBoxStart.y - dy, vb.width, vb.height);
      return;
    }

    if (this.isContainerDragging && this.containerDrag) {
      const currentPos = this._getSvgCoordinates(e);
      const dx = currentPos.x - this.mouseStartPos.x;
      const dy = currentPos.y - this.mouseStartPos.y;
      if (Math.abs(dx) > 1 || Math.abs(dy) > 1) {
        this.hasMoved = true;
        this._hideTooltip();
        this._moveContainerBy(dx, dy);
      }
      return;
    }

    if (this.isDragging && this.selectedElement) {
      const currentPos = this._getSvgCoordinates(e);
      const dx = currentPos.x - this.mouseStartPos.x;
      const dy = currentPos.y - this.mouseStartPos.y;
      if (Math.abs(dx) > 1 || Math.abs(dy) > 1) {
        this.hasMoved = true;
        this._hideTooltip();
        // 坐标钳制为非负：与坐标模态框"拒绝负值"口径一致，
        // 防止拖出画布左上后负坐标直接入库
        const newX = Math.max(0, this.elementStartPos.x + dx);
        const newY = Math.max(0, this.elementStartPos.y + dy);
        this._setElementPosition(this.selectedElement, newX, newY);
        if (this.callbacks.onNodeDrag) {
          this.callbacks.onNodeDrag(this.selectedElement.dataset.deviceId);
        }
      }
      return;
    }

    if (this.isConnectionMode && this.connectionSource) {
      this._hideTooltip();
      const svgPos = this._getSvgCoordinates(e);
      this._drawTempConnection(
        this.connectionSource.x,
        this.connectionSource.y,
        svgPos.x,
        svgPos.y
      );
      return;
    }

    this._updateTooltip(e);
  }

  _updateTooltip(e) {
    if (!this.tooltip) {
      return;
    }
    const target = e.target.closest("[data-tooltip]");
    if (!target || !target.dataset.tooltip) {
      this._hideTooltip();
      return;
    }

    this.tooltip.textContent = target.dataset.tooltip;
    this.tooltip.classList.add("visible");

    // 按鼠标在画布内的象限决定浮窗展开方向，使其始终朝画布内侧显示
    const TOOLTIP_MARGIN = 12;
    const canvasRect = this.container.getBoundingClientRect();
    const onLeftHalf = e.clientX - canvasRect.left < canvasRect.width / 2;
    const onTopHalf = e.clientY - canvasRect.top < canvasRect.height / 2;
    const left = onLeftHalf
      ? e.clientX + TOOLTIP_MARGIN
      : e.clientX - this.tooltip.offsetWidth - TOOLTIP_MARGIN;
    const top = onTopHalf
      ? e.clientY + TOOLTIP_MARGIN
      : e.clientY - this.tooltip.offsetHeight - TOOLTIP_MARGIN;

    this.tooltip.style.left = `${Math.max(8, Math.min(left, window.innerWidth - this.tooltip.offsetWidth - 8))}px`;
    this.tooltip.style.top = `${Math.max(8, Math.min(top, window.innerHeight - this.tooltip.offsetHeight - 8))}px`;
  }

  _hideTooltip() {
    if (this.tooltip) {
      this.tooltip.classList.remove("visible");
    }
  }

  _handleMouseUp() {
    if (this.isPanning) {
      this.isPanning = false;
      this.container.classList.remove("panning");
      return;
    }

    if (this.isContainerDragging && this.containerDrag) {
      const drag = this.containerDrag;
      this.isContainerDragging = false;
      this.containerDrag = null;
      if (this.hasMoved) {
        drag.memberStart.forEach(({ el }) => this._snapElementToGrid(el));
        if (this.callbacks.onContainerDragEnd) {
          this.callbacks.onContainerDragEnd(drag.fixedKey, drag.fixedGroupKey);
        }
      } else if (this.callbacks.onContainerClick) {
        // 容器单击（未拖动）：与设备一致打开模态框，透传当前包围盒坐标
        const rect = drag.groupEl.querySelector("rect");
        const box = rect
          ? {
              x: parseFloat(rect.getAttribute("x")),
              y: parseFloat(rect.getAttribute("y")),
              width: parseFloat(rect.getAttribute("width")),
              height: parseFloat(rect.getAttribute("height"))
            }
          : null;
        this.callbacks.onContainerClick(
          drag.groupEl.dataset.containerKind,
          drag.groupEl.dataset.containerKey,
          box
        );
      }
      this.hasMoved = false;
      return;
    }

    if (this.isDragging && !this.hasMoved && this.selectedElement) {
      if (this.callbacks.onNodeClick) {
        this.callbacks.onNodeClick(this.selectedElement.dataset.deviceId);
      }
    }

    if (this.isDragging && this.hasMoved && this.selectedElement) {
      this._snapElementToGrid(this.selectedElement);
      if (this.callbacks.onNodeDragEnd) {
        this.callbacks.onNodeDragEnd(this.selectedElement.dataset.deviceId);
      }
    }

    this.isDragging = false;
  }

  _handleWheel(e) {
    e.preventDefault();
    this._hideTooltip();
    const delta = e.deltaY > 0 ? 1.1 : 0.9;
    const vb = this.svg.viewBox.baseVal;
    const mousePos = this._getSvgCoordinates(e);

    const newWidth = vb.width * delta;
    const newHeight = vb.height * delta;

    if (newWidth < 300 || newWidth > 30000) {
      return;
    }

    const newX = mousePos.x - (mousePos.x - vb.x) * delta;
    const newY = mousePos.y - (mousePos.y - vb.y) * delta;

    this.setViewBox(newX, newY, newWidth, newHeight);
    this._updateZoomIndicator();
  }

  /** 当前显示比例：容器像素宽 / viewBox 宽（1 = 100%）。 */
  getZoomScale() {
    const vb = this.svg.viewBox.baseVal;
    const containerWidth = this.container.clientWidth || 1;
    return containerWidth / vb.width;
  }

  /**
   * 设置画布显示比例（配合平移可实现特定区域的局部放大/缩小）。
   * 以当前视野中心为锚点缩放，视野宽高比保持不变。
   */
  setZoom(scale) {
    const clamped = Math.max(0.05, Math.min(8, scale));
    const vb = this.svg.viewBox.baseVal;
    const centerX = vb.x + vb.width / 2;
    const centerY = vb.y + vb.height / 2;
    const containerWidth = this.container.clientWidth || 800;
    const newWidth = containerWidth / clamped;
    const newHeight = newWidth * (vb.height / vb.width);

    if (newWidth < 300 || newWidth > 30000) {
      return;
    }

    this.setViewBox(centerX - newWidth / 2, centerY - newHeight / 2, newWidth, newHeight);
    this._updateZoomIndicator();
  }

  /** 按倍率缩放（工具栏 +/- 按钮使用）。 */
  zoomBy(factor) {
    this.setZoom(this.getZoomScale() * factor);
  }

  _handleAnchorClick(anchor, e) {
    e.stopPropagation();
    const deviceId = anchor.closest("[data-device-id]").dataset.deviceId;
    const portDir = anchor.dataset.portDir;
    const portId = anchor.dataset.portId || null;

    const anchorPos = {
      x: parseFloat(anchor.getAttribute("cx")),
      y: parseFloat(anchor.getAttribute("cy"))
    };

    if (!this.connectionSource) {
      this.connectionSource = { deviceId, portId, portDir, x: anchorPos.x, y: anchorPos.y };
      anchor.classList.add("connection-source");
      return;
    }

    if (this.connectionSource.deviceId === deviceId && this.connectionSource.portId === portId) {
      this._cancelConnection();
      return;
    }

    // 同一设备两个端口互连属自环：与模态框创建入口（visualizationManager）校验一致，
    // 拒绝并提示
    if (this.connectionSource.deviceId === deviceId) {
      showToast(t("viz.no_self_connection"), "warning");
      this._cancelConnection();
      return;
    }

    if (this.callbacks.onConnectionComplete) {
      this.callbacks.onConnectionComplete(
        this.connectionSource.deviceId,
        this.connectionSource.portId,
        deviceId,
        portId
      );
    }

    this._cancelConnection();
  }

  _cancelConnection() {
    this.connectionSource = null;
    this.tempConnectionGroup.innerHTML = "";
    this.elementsGroup
      .querySelectorAll(".connection-source")
      .forEach((a) => a.classList.remove("connection-source"));
  }

  _drawTempConnection(x1, y1, x2, y2) {
    this.tempConnectionGroup.innerHTML = "";
    const line = document.createElementNS(SVG_NS, "path");
    const dx = x2 - x1;
    const dy = y2 - y1;
    const dist = Math.sqrt(dx * dx + dy * dy);
    const offset = Math.min(dist * 0.3, 80);
    line.setAttribute(
      "d",
      `M ${x1} ${y1} C ${x1 + offset} ${y1}, ${x2 - offset} ${y2}, ${x2} ${y2}`
    );
    line.setAttribute("class", "temp-connection");
    this.tempConnectionGroup.appendChild(line);
  }

  _setElementPosition(element, x, y) {
    const rect = element.querySelector("rect");
    if (!rect) {
      return;
    }

    const width = parseFloat(rect.getAttribute("width")) || 200;

    rect.setAttribute("x", x);
    rect.setAttribute("y", y);

    element.querySelectorAll("text").forEach((text) => {
      const relX = parseFloat(text.dataset.relX || 0);
      const relY = parseFloat(text.dataset.relY || 0);
      text.setAttribute("x", x + width / 2 + relX);
      text.setAttribute("y", y + relY);
    });

    // 设备类型图标（相对节点左上角定位，随拖动平移）
    element.querySelectorAll(".device-icon").forEach((icon) => {
      const relX = parseFloat(icon.dataset.relX || 0);
      const relY = parseFloat(icon.dataset.relY || 0);
      icon.setAttribute("transform", `translate(${x + relX}, ${y + relY})`);
    });

    element.querySelectorAll(".port-anchor").forEach((anchor) => {
      const relCx = parseFloat(anchor.dataset.relCx || 0);
      const relCy = parseFloat(anchor.dataset.relCy || 0);
      anchor.setAttribute("cx", x + relCx);
      anchor.setAttribute("cy", y + relCy);
    });
  }

  /**
   * 开始容器拖动：按容器类型匹配组内全部设备节点并记录起始位置。
   * @returns {boolean} 是否成功进入容器拖动（无成员时返回 false 走默认点击逻辑）
   */
  _startContainerDrag(containerG, e) {
    const kind = containerG.dataset.containerKind;
    const key = containerG.dataset.containerKey;
    if (!kind || !key) {
      return false;
    }

    const members = [...this.elementsGroup.querySelectorAll("[data-device-id]")].filter((el) =>
      kind === "cabinet" ? el.dataset.cabinetKey === key : el.dataset.roomKey === key
    );
    if (members.length === 0) {
      return false;
    }

    const rect = containerG.querySelector("rect");
    if (!rect) {
      return false;
    }

    this.isContainerDragging = true;
    this.hasMoved = false;
    // 推挤分组时被拖容器保持不动：房间容器即房间键，机柜容器取首个成员的房间键；
    // 机柜级推挤还需固定被拖机柜自身的分组键（cab:...），否则落定后被对称推移
    const fixedKey = kind === "room" ? key : members[0].dataset.roomKey || "room:none";
    const fixedGroupKey = kind === "cabinet" ? key : null;
    this.containerDrag = {
      groupEl: containerG,
      rectStart: {
        x: parseFloat(rect.getAttribute("x")),
        y: parseFloat(rect.getAttribute("y"))
      },
      memberStart: members.map((el) => {
        const r = el.querySelector("rect");
        return {
          el,
          x: parseFloat(r.getAttribute("x")),
          y: parseFloat(r.getAttribute("y"))
        };
      }),
      textsStart: [...containerG.querySelectorAll("text")].map((text) => ({
        text,
        x: parseFloat(text.getAttribute("x")),
        y: parseFloat(text.getAttribute("y"))
      })),
      fixedKey,
      fixedGroupKey
    };
    this.mouseStartPos = this._getSvgCoordinates(e);
    return true;
  }

  /** 容器拖动位移：从起始位置整体平移组内节点与容器框/文字 */
  _moveContainerBy(dx, dy) {
    const drag = this.containerDrag;
    const rect = drag.groupEl.querySelector("rect");
    rect.setAttribute("x", drag.rectStart.x + dx);
    rect.setAttribute("y", drag.rectStart.y + dy);
    drag.textsStart.forEach(({ text, x, y }) => {
      text.setAttribute("x", x + dx);
      text.setAttribute("y", y + dy);
    });
    drag.memberStart.forEach(({ el, x, y }) => {
      this._setElementPosition(el, x + dx, y + dy);
    });
    this._scheduleContainerDragRefresh();
  }

  /** 容器拖拽的连线重画合并到每帧一次：成员多时避免 N 次/帧的重画请求 */
  _scheduleContainerDragRefresh() {
    if (this._containerDragFrame) {
      return;
    }
    this._containerDragFrame = requestAnimationFrame(() => {
      this._containerDragFrame = null;
      if (this.callbacks.onNodeDrag) {
        // 空 deviceId 表示全量重画（一次覆盖全部成员的位移）
        this.callbacks.onNodeDrag(null);
      }
    });
  }

  _snapElementToGrid(element) {
    const rect = element.querySelector("rect");
    if (!rect) {
      return;
    }
    const x = parseFloat(rect.getAttribute("x"));
    const y = parseFloat(rect.getAttribute("y"));
    const snappedX = Math.round(x / this.gridSize) * this.gridSize;
    const snappedY = Math.round(y / this.gridSize) * this.gridSize;
    if (snappedX !== x || snappedY !== y) {
      this._setElementPosition(element, snappedX, snappedY);
    }
  }

  _selectElement(element) {
    this._clearSelection();
    this.selectedElement = element;
    element.classList.add("selected");
  }

  _clearSelection() {
    this.elementsGroup
      .querySelectorAll(".selected")
      .forEach((el) => el.classList.remove("selected"));
    this.selectedElement = null;
  }

  _getSvgCoordinates(e) {
    const pt = this.svg.createSVGPoint();
    const rect = this.svg.getBoundingClientRect();
    pt.x = e.clientX - rect.left;
    pt.y = e.clientY - rect.top;
    const ctm = this.svg.getScreenCTM().inverse();
    return pt.matrixTransform(ctm);
  }

  /**
   * 同步工具栏显示比例控件：工具栏为下拉选择（#topology-zoom-level），
   * 命中预设档位（±2% 容差）则选中对应项，否则补充/更新一个动态档位项。
   */
  _updateZoomIndicator() {
    const indicator = document.getElementById("topology-zoom-level");
    if (!indicator) {
      return;
    }
    if (indicator.tagName !== "SELECT") {
      indicator.textContent = `${Math.round(this.getZoomScale() * 100)}%`;
      return;
    }

    const scale = this.getZoomScale();
    const preset = [...indicator.options].find(
      (opt) => opt.value !== "fit" && Math.abs(parseFloat(opt.value) - scale) / scale <= 0.02
    );

    const dynamicId = "zoom-current";
    const dynamic = indicator.querySelector(`option[value="${dynamicId}"]`);
    if (preset) {
      dynamic?.remove();
      indicator.value = preset.value;
    } else {
      if (!dynamic) {
        const opt = document.createElement("option");
        opt.value = dynamicId;
        indicator.insertBefore(opt, indicator.firstChild);
      }
      const opt = indicator.querySelector(`option[value="${dynamicId}"]`);
      opt.textContent = `${Math.round(scale * 100)}%`;
      indicator.value = dynamicId;
    }
  }

  setConnectionMode(enabled) {
    this.isConnectionMode = enabled;
    if (!enabled) {
      this._cancelConnection();
    }
    this.container.classList.toggle("connection-mode", enabled);
  }

  getNodePosition(deviceId) {
    const element = this.elementsGroup.querySelector(`[data-device-id="${CSS.escape(deviceId)}"]`);
    if (!element) {
      return null;
    }
    const rect = element.querySelector("rect");
    if (!rect) {
      return null;
    }
    return {
      x: parseFloat(rect.getAttribute("x")),
      y: parseFloat(rect.getAttribute("y")),
      width: parseFloat(rect.getAttribute("width")),
      height: parseFloat(rect.getAttribute("height"))
    };
  }

  getAnchorPosition(deviceId, direction) {
    const pos = this.getNodePosition(deviceId);
    if (!pos) {
      return null;
    }
    switch (direction) {
      case "top":
        return { x: pos.x + pos.width / 2, y: pos.y };
      case "right":
        return { x: pos.x + pos.width, y: pos.y + pos.height / 2 };
      case "bottom":
        return { x: pos.x + pos.width / 2, y: pos.y + pos.height };
      case "left":
        return { x: pos.x, y: pos.y + pos.height / 2 };
      default:
        return { x: pos.x + pos.width / 2, y: pos.y + pos.height / 2 };
    }
  }
}
