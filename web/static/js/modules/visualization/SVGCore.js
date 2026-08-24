import { apiGet, apiPost, apiDelete } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";
import { showConfirm } from "../../utils/confirm.js";

export class SVGCore {
  constructor(containerId, type, callbacks = {}) {
    this.container = document.getElementById(containerId);
    this.type = type;
    this.svg = null;
    this.elementsGroup = null;
    this.selectedElement = null;
    this.isDragging = false;
    this.hasMoved = false;
    this.elementStartPos = { x: 0, y: 0 };
    this.mouseStartPos = { x: 0, y: 0 };
    this.currentRoomId = null;
    this.currentCabinetId = null;
    this.gridSize = 20;
    this.snapToGrid = true;
    this.showAlignmentLines = true;
    this.alignmentThreshold = 10;
    this.alignmentLinesGroup = null;
    this.tooltip = null;
    this.callbacks = callbacks;
    this.apiGet = apiGet;
    this.apiPost = apiPost;
    this.apiDelete = apiDelete;
    this.showToast = showToast;
    this.showConfirm = showConfirm;

    this._init();
  }

  _init() {
    this._initSVG();
    this._initTooltip();
    this._initEventListeners();
  }

  _initSVG() {
    this.container.innerHTML = "";

    this.svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    this.svg.className.baseVal = "visualization-svg";
    this.svg.setAttribute("width", "100%");
    this.svg.setAttribute("height", "100%");
    this.svg.setAttribute("viewBox", "0 0 2000 2000");
    if (this.type === "cabinet") {
      // 机柜视图内容锚定画布底部，保证不同窗口/分辨率下柜底始终贴近屏幕底部；
      // 初始 viewBox 直接取容器尺寸，避免先渲染 2000x2000 再纠正的闪变
      this.svg.setAttribute("preserveAspectRatio", "xMidYMax meet");
      const w = this.container.clientWidth || 800;
      const h = this.container.clientHeight || 600;
      this.svg.setAttribute("viewBox", `0 0 ${w} ${h}`);
    }

    this._createDefs();
    this._createGridBackground();
    this._createGridRulerGroup();

    this.elementsGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.svg.appendChild(this.elementsGroup);

    this.alignmentLinesGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.alignmentLinesGroup.className.baseVal = "alignment-lines";
    this.svg.appendChild(this.alignmentLinesGroup);

    this.container.appendChild(this.svg);

    // 尺寸与边框由 CSS（visualization.css）控制，禁止内联 maxHeight 限制容器高度，
    // 否则高分辨率屏幕下机柜底部无法贴近屏幕底部
    this.container.style.overflow = "auto";
  }

  _createDefs() {
    const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");

    const gridPattern = document.createElementNS("http://www.w3.org/2000/svg", "pattern");
    const gridId = `grid-${this.type}`;
    gridPattern.setAttribute("id", gridId);
    gridPattern.setAttribute("width", this.gridSize);
    gridPattern.setAttribute("height", this.gridSize);
    gridPattern.setAttribute("patternUnits", "userSpaceOnUse");

    const gridPath = document.createElementNS("http://www.w3.org/2000/svg", "path");
    gridPath.setAttribute("d", `M ${this.gridSize} 0 L 0 0 0 ${this.gridSize}`);
    gridPath.setAttribute("fill", "none");
    gridPath.setAttribute("stroke", "var(--border-light)");
    gridPath.setAttribute("stroke-width", "0.5");

    gridPattern.appendChild(gridPath);
    defs.appendChild(gridPattern);

    this.svg.appendChild(defs);
  }

  _createGridBackground() {
    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("width", "100%");
    rect.setAttribute("height", "100%");
    rect.setAttribute("fill", `url(#grid-${this.type})`);
    this.gridRect = rect;
    this.svg.appendChild(rect);
  }

  /**
   * 主网格线 + 坐标标尺层：位于元素层之下。
   * 主网格间距为 gridSize 的 5 倍（100px），每条主网格线在视口
   * 顶边/左边标注画布坐标，配合坐标输入框精确定位。
   */
  _createGridRulerGroup() {
    this.gridRulerGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.gridRulerGroup.className.baseVal = "grid-ruler";
    this.svg.appendChild(this.gridRulerGroup);
    this._renderGridRuler();
  }

  /** 按当前 viewBox 重绘主网格与坐标标注（viewBox 变化后调用）。 */
  _renderGridRuler() {
    if (!this.gridRulerGroup) return;
    const vb = this.svg.viewBox.baseVal;
    const group = this.gridRulerGroup;
    group.innerHTML = "";
    if (!vb.width || !vb.height) return;

    const SVG_NS = "http://www.w3.org/2000/svg";
    const step = this.gridSize * 5;
    // 标注字号随视口宽度缩放，缩放后保持可读
    const fontSize = Math.max(9, Math.min(14, vb.width / 100));
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

  /** 统一的 viewBox 更新入口：同步重绘主网格与坐标标注。 */
  setViewBox(x, y, width, height) {
    this.svg.setAttribute("viewBox", `${x} ${y} ${width} ${height}`);
    this._renderGridRuler();
  }

  _initTooltip() {
    const tooltipId = `visualization-tooltip-${this.type}`;
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

  _initEventListeners() {
    this.svg.addEventListener("mousedown", this._handleMouseDown.bind(this));
    this.svg.addEventListener("mousemove", this._handleMouseMove.bind(this));
    this.svg.addEventListener("mouseup", this._handleMouseUp.bind(this));
    this.svg.addEventListener("mouseleave", () => {
      this._handleMouseUp();
      this._hideTooltip();
      this._clearAlignmentLines();
    });

    this.svg.addEventListener("click", (e) => {
      const isElement = e.target.closest("[data-id]");
      if (!isElement) {
        this._clearSelection();
      }
    });
  }

  _handleMouseDown(e) {
    if (e.button !== 0) return;
    const target = e.target.closest("[data-id]");
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
    }
  }

  _handleMouseMove(e) {
    if (this.isDragging && this.selectedElement) {
      const currentPos = this._getSvgCoordinates(e);
      const dx = currentPos.x - this.mouseStartPos.x;
      const dy = currentPos.y - this.mouseStartPos.y;

      if (Math.abs(dx) > 1 || Math.abs(dy) > 1) {
        this.hasMoved = true;

        const newX = this.elementStartPos.x + dx;
        const newY = this.elementStartPos.y + dy;

        this._setElementPosition(this.selectedElement, newX, newY);

        if (this.showAlignmentLines) {
          this._updateAlignmentLines(this.selectedElement);
        }

        this._hideTooltip();
      }
      return;
    }

    this._updateTooltip(e);
  }

  _handleMouseUp(e) {
    if (this.isDragging && !this.hasMoved && this.selectedElement) {
      this._handleElementClick(this.selectedElement);
    }

    if (this.isDragging && this.hasMoved && this.selectedElement && this.snapToGrid) {
      this._snapElementToGrid(this.selectedElement);
    }

    // 拖拽落定后通知上层（可视化层据此自动保存坐标，实现页面直接编辑位置）
    if (this.isDragging && this.hasMoved && this.selectedElement && this.callbacks.onPositionChanged) {
      this.callbacks.onPositionChanged(this.selectedElement.dataset.id);
    }

    this._clearAlignmentLines();
    this.isDragging = false;
  }

  _setElementPosition(element, x, y) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const width = parseFloat(rect.getAttribute("width")) || 160;
    const height = parseFloat(rect.getAttribute("height")) || 160;

    rect.setAttribute("x", x);
    rect.setAttribute("y", y);

    const texts = element.querySelectorAll("text");
    texts.forEach((text) => {
      const relX = parseFloat(text.dataset.relX || 0);
      const relY = parseFloat(text.dataset.relY || 0);
      text.setAttribute("x", x + width / 2 + relX);
      text.setAttribute("y", y + relY);
    });

    const lines = element.querySelectorAll("line");
    lines.forEach((line) => {
      const relX1 = parseFloat(line.dataset.relX1 || 0);
      const relY1 = parseFloat(line.dataset.relY1 || 0);
      const relX2 = parseFloat(line.dataset.relX2 || 0);
      const relY2 = parseFloat(line.dataset.relY2 || 0);
      line.setAttribute("x1", x + relX1);
      line.setAttribute("y1", y + relY1);
      line.setAttribute("x2", x + relX2);
      line.setAttribute("y2", y + relY2);
    });

    if (element.classList.contains("door-element")) {
      const doorHandle = element.querySelector("circle");
      if (doorHandle) {
        const relCx = parseFloat(doorHandle.dataset.relCx || 0);
        const relCy = parseFloat(doorHandle.dataset.relCy || 0);
        doorHandle.setAttribute("cx", x + relCx);
        doorHandle.setAttribute("cy", y + relCy);
      }
    }

    if (element.classList.contains("cabinet-element")) {
      this._updateChildPositions(element, x, y);
    }
  }

  _updateChildPositions(element, parentX, parentY) {
    const cabinetId = element.dataset.id;
    const positions = this.elementsGroup.querySelectorAll(".cabinet-position-element");
    positions.forEach((position) => {
      if (position.dataset.cabinetId === cabinetId) {
        const relX = parseFloat(position.dataset.relX || 0);
        const relY = parseFloat(position.dataset.relY || 0);
        const newX = parentX + relX;
        const newY = parentY + relY;

        const positionRect = position.querySelector("rect");
        if (positionRect) {
          positionRect.setAttribute("x", newX);
          positionRect.setAttribute("y", newY);
        }
        const positionTexts = position.querySelectorAll("text");
        positionTexts.forEach((text) => {
          const textRelX = parseFloat(text.dataset.relX || 0);
          const textRelY = parseFloat(text.dataset.relY || 0);
          text.setAttribute("x", newX + textRelX);
          text.setAttribute("y", newY + textRelY);
        });
      }
    });
  }

  _snapElementToGrid(element) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const x = parseFloat(rect.getAttribute("x"));
    const y = parseFloat(rect.getAttribute("y"));

    const snappedX = Math.round(x / this.gridSize) * this.gridSize;
    const snappedY = Math.round(y / this.gridSize) * this.gridSize;

    if (snappedX !== x || snappedY !== y) {
      this._setElementPosition(element, snappedX, snappedY);
    }
  }

  _handleElementClick(element) {
    const id = element.dataset.id;
    const elementType = element.classList.contains("workstation-element")
      ? "workstation"
      : element.classList.contains("cabinet-element")
        ? "cabinet"
        : element.classList.contains("cabinet-position-element")
          ? "cabinet-position"
          : "";

    // 画布当前坐标随事件透传（工位模态框坐标输入框回填用）
    const rect = element.querySelector("rect");
    const position = rect
      ? {
          x: parseFloat(rect.getAttribute("x")) || 0,
          y: parseFloat(rect.getAttribute("y")) || 0
        }
      : null;

    if (elementType === "workstation" && this.callbacks.onEditWorkstation) {
      this.callbacks.onEditWorkstation(id, position);
    } else if (elementType === "cabinet" && this.callbacks.onEditCabinet) {
      this.callbacks.onEditCabinet(id);
    } else if (elementType === "cabinet-position" && this.callbacks.onEditCabinetPosition) {
      this.callbacks.onEditCabinetPosition(id);
    }
  }

  _getSvgCoordinates(e) {
    const pt = this.svg.createSVGPoint();
    const rect = this.svg.getBoundingClientRect();
    pt.x = e.clientX - rect.left;
    pt.y = e.clientY - rect.top;
    const ctm = this.svg.getScreenCTM().inverse();
    return pt.matrixTransform(ctm);
  }

  _selectElement(element) {
    this._clearSelection();
    this.selectedElement = element;
    element.classList.add("selected");
  }

  _clearSelection() {
    const selected = this.elementsGroup.querySelector(".selected");
    if (selected) {
      selected.classList.remove("selected");
    }
    this.selectedElement = null;
  }

  _updateAlignmentLines(element) {
    this._clearAlignmentLines();

    const rect = element.querySelector("rect");
    if (!rect) return;

    const x = parseFloat(rect.getAttribute("x"));
    const y = parseFloat(rect.getAttribute("y"));
    const width = parseFloat(rect.getAttribute("width"));
    const height = parseFloat(rect.getAttribute("height"));

    const elementCenterX = x + width / 2;
    const elementCenterY = y + height / 2;

    const allElements = this.elementsGroup.querySelectorAll("[data-id]");
    const otherElements = Array.from(allElements).filter((el) => el !== element);

    otherElements.forEach((other) => {
      const otherRect = other.querySelector("rect");
      if (!otherRect) return;

      const ox = parseFloat(otherRect.getAttribute("x"));
      const oy = parseFloat(otherRect.getAttribute("y"));
      const ow = parseFloat(otherRect.getAttribute("width"));
      const oh = parseFloat(otherRect.getAttribute("height"));
      const otherCenterX = ox + ow / 2;
      const otherCenterY = oy + oh / 2;

      if (Math.abs(elementCenterX - otherCenterX) < this.alignmentThreshold) {
        this._drawAlignmentLine(
          otherCenterX,
          Math.min(y, oy),
          otherCenterX,
          Math.max(y + height, oy + oh)
        );
      }

      if (Math.abs(elementCenterY - otherCenterY) < this.alignmentThreshold) {
        this._drawAlignmentLine(
          Math.min(x, ox),
          otherCenterY,
          Math.max(x + width, ox + ow),
          otherCenterY
        );
      }

      if (Math.abs(x - ox) < this.alignmentThreshold) {
        this._drawAlignmentLine(ox, Math.min(y, oy), ox, Math.max(y + height, oy + oh));
      }

      if (Math.abs(y - oy) < this.alignmentThreshold) {
        this._drawAlignmentLine(Math.min(x, ox), oy, Math.max(x + width, ox + ow), oy);
      }
    });
  }

  _drawAlignmentLine(x1, y1, x2, y2) {
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", x1);
    line.setAttribute("y1", y1);
    line.setAttribute("x2", x2);
    line.setAttribute("y2", y2);
    line.setAttribute("stroke", "#3b82f6");
    line.setAttribute("stroke-width", "1");
    line.setAttribute("stroke-dasharray", "4,4");
    line.setAttribute("opacity", "0.7");
    this.alignmentLinesGroup.appendChild(line);
  }

  _clearAlignmentLines() {
    if (this.alignmentLinesGroup) {
      this.alignmentLinesGroup.innerHTML = "";
    }
  }

  _updateTooltip(e) {
    if (!this.tooltip) return;
    const target = e.target.closest("[data-tooltip]");
    if (!target || !target.dataset.tooltip) {
      this._hideTooltip();
      return;
    }

    this.tooltip.textContent = target.dataset.tooltip;
    this.tooltip.classList.add("visible");

    // 按鼠标在画布内的象限决定浮窗展开方向，使其始终朝画布内侧显示：
    // 鼠标位于左半/上半时浮窗放右下，位于右半/下半时放左上，以此类推
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

  setGridSize(size) {
    this.gridSize = size;
    this._createDefs();
    this._createGridBackground();
    this._renderGridRuler();
  }

  /**
   * 机柜画布定型（虚拟横向滚动方案）。
   *
   * Chromium 无法光栅化超大宽度的 SVG 元素（如千柜 ~20 万像素），因此 SVG
   * 元素始终保持视口尺寸，由占位元素撑出内容总宽度提供原生横向滚动条；
   * 滚动时平移 viewBox 的 x 偏移，内容坐标仍按全局 1:1 布局，拖拽与对齐
   * 逻辑经 getScreenCTM 自动适配。
   * 必须在绘制机柜之前调用，保证首帧就是最终布局。
   */
  setCabinetCanvasSize(totalWidth, height) {
    if (!this._cabinetScrollBound) {
      this._cabinetScrollBound = true;
      this._cabinetViewHeight = height;
      // rAF 节流：滚动时仅更新 viewBox 与网格位置
      this.container.addEventListener("scroll", () => {
        if (this._cabinetScrollRaf) return;
        this._cabinetScrollRaf = requestAnimationFrame(() => {
          this._cabinetScrollRaf = 0;
          this._applyCabinetScroll(this.container.scrollLeft);
        });
      });
    }
    this._cabinetViewHeight = height;

    // 占位元素绝对定位需要一个定位上下文
    if (!this.container.style.position) {
      this.container.style.position = "relative";
    }

    if (!this.scrollSpacer || !this.scrollSpacer.isConnected) {
      const spacer = document.createElement("div");
      spacer.style.cssText = "position:absolute;top:0;left:0;height:1px;pointer-events:none;";
      this.container.appendChild(spacer);
      this.scrollSpacer = spacer;
    }
    this.scrollSpacer.style.width = `${totalWidth}px`;

    // SVG 覆盖在占位元素之上并吸住视口左缘
    this.svg.style.position = "sticky";
    this.svg.style.left = "0";
    this.svg.style.width = "100%";
    this.svg.setAttribute("height", "100%");

    this._applyCabinetScroll(this.container.scrollLeft);
  }

  /** 按横向滚动偏移更新机柜画布 viewBox 与网格窗口。 */
  _applyCabinetScroll(scrollLeft) {
    const viewWidth = this.container.clientWidth || 800;
    this.setViewBox(scrollLeft, 0, viewWidth, this._cabinetViewHeight || 600);
    if (this.gridRect) {
      this.gridRect.setAttribute("x", scrollLeft);
      this.gridRect.setAttribute("width", viewWidth);
    }
  }

  toggleSnapToGrid(enabled) {
    this.snapToGrid = enabled;
  }

  toggleAlignmentLines(enabled) {
    this.showAlignmentLines = enabled;
    if (!enabled) {
      this._clearAlignmentLines();
    }
  }
}
