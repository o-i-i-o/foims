/**
 * SVG 画布交互基类：SVGCore（工位/机柜布局）与 TopologyCore（拓扑）共享的
 * 网格/标尺/viewBox/tooltip/坐标换算/吸附/选中实现，避免两份复制漂移。
 *
 * 子类职责：
 * - 在构造器末尾调用 this._init()（基类构造器不自动初始化，
 *   保证子类自身的字段先就位）；
 * - 实现 _initSVG / _initEventListeners（各自画布结构与交互手势不同）；
 * - 按需覆写 _onElementPositioned（元素位移后的联动，如机柜子元素跟随）。
 */

// SVG 命名空间（visualization 各模块共用，唯一定义处；SVGCore.js 再导出）
export const SVG_NS = "http://www.w3.org/2000/svg";

export class SVGCanvasBase {
  /**
   * @param {string} containerId 容器元素 id
   * @param {Object} options
   * @param {string} options.type 画布类型（tooltip id 与 pattern id 的组成部分）
   * @param {Object} options.callbacks 事件回调集合
   * @param {number} [options.defaultElementWidth] rect 宽度缺失时的回退值
   * @param {number} [options.rulerFontDivisor] 标尺字号随视口宽度的缩放分母
   * @param {string} [options.gridStroke] 网格线颜色（建议 CSS 变量以跟随主题）
   */
  constructor(
    containerId,
    {
      type,
      callbacks = {},
      defaultElementWidth = 160,
      rulerFontDivisor = 100,
      gridStroke = "var(--border-light)"
    }
  ) {
    this.container = document.getElementById(containerId);
    this.type = type;
    this.svg = null;
    this.elementsGroup = null;
    this.selectedElement = null;
    this.isDragging = false;
    this.hasMoved = false;
    this.elementStartPos = { x: 0, y: 0 };
    this.mouseStartPos = { x: 0, y: 0 };
    this.gridSize = 20;
    this.tooltip = null;
    this.callbacks = callbacks;
    this.defaultElementWidth = defaultElementWidth;
    this.rulerFontDivisor = rulerFontDivisor;
    this.gridStroke = gridStroke;
    this.gridPatternId = `grid-${this.type}`;
  }

  /** 初始化模板：SVG 结构 → tooltip → 事件（子类构造器末尾调用）。 */
  _init() {
    this._initSVG();
    this._initTooltip();
    this._initEventListeners();
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

  _createDefs() {
    // 重建时先移除旧 defs，避免产生重复的 pattern id
    this.svg.querySelector("defs")?.remove();
    const defs = document.createElementNS(SVG_NS, "defs");

    const gridPattern = document.createElementNS(SVG_NS, "pattern");
    gridPattern.setAttribute("id", this.gridPatternId);
    gridPattern.setAttribute("width", this.gridSize);
    gridPattern.setAttribute("height", this.gridSize);
    gridPattern.setAttribute("patternUnits", "userSpaceOnUse");

    const gridPath = document.createElementNS(SVG_NS, "path");
    gridPath.setAttribute("d", `M ${this.gridSize} 0 L 0 0 0 ${this.gridSize}`);
    gridPath.setAttribute("fill", "none");
    gridPath.setAttribute("stroke", this.gridStroke);
    gridPath.setAttribute("stroke-width", "0.5");

    gridPattern.appendChild(gridPath);
    defs.appendChild(gridPattern);

    this.svg.appendChild(defs);
  }

  /**
   * 按当前 viewBox 重绘主网格与坐标标注（viewBox 变化后调用）。
   * 主网格间距为 gridSize 的 5 倍（100px），每条主网格线在视口
   * 顶边/左边标注画布坐标，配合坐标输入框精确定位。
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
    const fontSize = Math.max(9, Math.min(14, vb.width / this.rulerFontDivisor));
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

  /** 统一的 viewBox 更新入口：背景矩形同步铺满，rAF 节流重绘主网格与坐标标注（平移/缩放/ResizeObserver 高频触发）。 */
  setViewBox(x, y, width, height) {
    this.svg.setAttribute("viewBox", `${x} ${y} ${width} ${height}`);
    // 背景矩形跟随 viewBox 铺满可视区域（pattern 为 userSpaceOnUse，坐标不受影响）
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

  _getSvgCoordinates(e) {
    const pt = this.svg.createSVGPoint();
    const rect = this.svg.getBoundingClientRect();
    pt.x = e.clientX - rect.left;
    pt.y = e.clientY - rect.top;
    const ctm = this.svg.getScreenCTM().inverse();
    return pt.matrixTransform(ctm);
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

  /**
   * 统一的元素位移：按 rect 定位，text/line/circle 子元素按各自的
   * data-rel* 相对坐标跟随（工位画布的门窗元素与拓扑的图标/端口锚点
   * 共用同一相对坐标约定），位移后触发 _onElementPositioned 联动钩子。
   */
  _setElementPosition(element, x, y) {
    const rect = element.querySelector("rect");
    if (!rect) {
      return;
    }

    const width = parseFloat(rect.getAttribute("width")) || this.defaultElementWidth;

    rect.setAttribute("x", x);
    rect.setAttribute("y", y);

    element.querySelectorAll("text").forEach((text) => {
      const relX = parseFloat(text.dataset.relX || 0);
      const relY = parseFloat(text.dataset.relY || 0);
      text.setAttribute("x", x + width / 2 + relX);
      text.setAttribute("y", y + relY);
    });

    element.querySelectorAll("line").forEach((line) => {
      const relX1 = parseFloat(line.dataset.relX1 || 0);
      const relY1 = parseFloat(line.dataset.relY1 || 0);
      const relX2 = parseFloat(line.dataset.relX2 || 0);
      const relY2 = parseFloat(line.dataset.relY2 || 0);
      line.setAttribute("x1", x + relX1);
      line.setAttribute("y1", y + relY1);
      line.setAttribute("x2", x + relX2);
      line.setAttribute("y2", y + relY2);
    });

    // 圆形子元素（门把手 / 拓扑端口锚点）按 relCx/relCy 跟随
    element.querySelectorAll("circle").forEach((circle) => {
      const relCx = parseFloat(circle.dataset.relCx || 0);
      const relCy = parseFloat(circle.dataset.relCy || 0);
      circle.setAttribute("cx", x + relCx);
      circle.setAttribute("cy", y + relCy);
    });

    // 拓扑设备类型图标按相对坐标以 transform 平移
    element.querySelectorAll(".device-icon").forEach((icon) => {
      const relX = parseFloat(icon.dataset.relX || 0);
      const relY = parseFloat(icon.dataset.relY || 0);
      icon.setAttribute("transform", `translate(${x + relX}, ${y + relY})`);
    });

    this._onElementPositioned(element, x, y);
  }

  /** 元素位移后的联动钩子（基类空实现；机柜子元素跟随等由子类覆写）。 */
  _onElementPositioned(_element, _x, _y) {}
}
