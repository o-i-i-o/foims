import { apiGet, apiPost, apiDelete } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";
import { showConfirm } from "../../utils/confirm.js";
import { SVGCanvasBase, SVG_NS } from "./svgCanvasBase.js";

// SVG 命名空间（visualization 各模块共用，唯一定义处）
export { SVG_NS };

/**
 * 工位/机柜布局画布：网格背景、元素拖拽/吸附/对齐线与机柜虚拟横向滚动。
 * 网格/标尺/viewBox/tooltip/坐标换算等共享实现在 SVGCanvasBase。
 */
export class SVGCore extends SVGCanvasBase {
  constructor(containerId, type, callbacks = {}) {
    super(containerId, { type, callbacks, defaultElementWidth: 160, rulerFontDivisor: 100 });
    this.currentRoomId = null;
    this.currentCabinetId = null;
    this.snapToGrid = true;
    this.showAlignmentLines = true;
    this.alignmentThreshold = 10;
    this.alignmentLinesGroup = null;
    this.apiGet = apiGet;
    this.apiPost = apiPost;
    this.apiDelete = apiDelete;
    this.showToast = showToast;
    this.showConfirm = showConfirm;

    this._init();
  }

  _initSVG() {
    this.container.innerHTML = "";

    this.svg = document.createElementNS(SVG_NS, "svg");
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
    } else {
      // 工位视图内容锚定左上角：默认 xMidYMid 会在容器宽高比与 viewBox 不一致时
      // 把网格整体居中，左缘留出空白带，视觉上画布与左侧边栏之间偏离一段距离。
      // 初始 viewBox 取容器尺寸（下限 1000x800），网格从画布左缘铺起
      this.svg.setAttribute("preserveAspectRatio", "xMinYMin meet");
      const w = this.container.clientWidth || 0;
      const h = this.container.clientHeight || 0;
      this.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, w)} ${Math.max(800, h)}`);
    }

    this._createDefs();
    this._createGridBackground();
    this._createGridRulerGroup();

    this.elementsGroup = document.createElementNS(SVG_NS, "g");
    this.svg.appendChild(this.elementsGroup);

    this.alignmentLinesGroup = document.createElementNS(SVG_NS, "g");
    this.alignmentLinesGroup.className.baseVal = "alignment-lines";
    this.svg.appendChild(this.alignmentLinesGroup);

    this.container.appendChild(this.svg);

    // 尺寸与边框由 CSS（visualization.css）控制，禁止内联 maxHeight 限制容器高度，
    // 否则高分辨率屏幕下机柜底部无法贴近屏幕底部
    this.container.style.overflow = "auto";

    // 工位画布：容器从隐藏变可见（切换页面/子标签）、侧边栏折叠、窗口缩放时
    // 保证 viewBox 至少覆盖容器，网格背景铺满画布
    if (this.type !== "cabinet" && typeof ResizeObserver !== "undefined") {
      this._containerObserver = new ResizeObserver(() => this.fitViewBoxToContainer());
      this._containerObserver.observe(this.container);
    }
  }

  /**
   * 工位画布尺寸跟踪：viewBox 至少覆盖容器尺寸，保证网格背景铺满画布；
   * 已有内容时保留内容边界（取两者最大值），只增不减。
   */
  fitViewBoxToContainer() {
    const w = this.container.clientWidth || 0;
    const h = this.container.clientHeight || 0;
    if (!w || !h) {
      return;
    }
    const vb = this.svg.viewBox.baseVal;
    if (vb.width >= w && vb.height >= h) {
      return;
    }
    this.setViewBox(0, 0, Math.max(vb.width, w), Math.max(vb.height, h));
  }

  /**
   * 网格背景矩形。宽度/高度不能用百分比：百分比按 SVG 视口（容器像素）
   * 而非 viewBox 解析，宽屏下 viewBox 长宽比与容器不一致时网格只覆盖
   * 画布局部区域，因此这里显式按初始 viewBox 尺寸铺满，并在 setViewBox
   * 中同步更新。
   */
  _createGridBackground() {
    this.gridRect?.remove();
    const rect = document.createElementNS(SVG_NS, "rect");
    const vb = this.svg.viewBox.baseVal;
    rect.setAttribute("x", vb.x);
    rect.setAttribute("y", vb.y);
    rect.setAttribute("width", vb.width);
    rect.setAttribute("height", vb.height);
    rect.setAttribute("fill", `url(#${this.gridPatternId})`);
    this.gridRect = rect;
    // 保持图层顺序：背景位于标尺层与元素层之下
    this.svg.insertBefore(rect, this.gridRulerGroup ?? null);
  }

  /**
   * 主网格线 + 坐标标尺层：位于元素层之下。
   * 主网格间距为 gridSize 的 5 倍（100px），每条主网格线在视口
   * 顶边/左边标注画布坐标，配合坐标输入框精确定位。
   */
  _createGridRulerGroup() {
    this.gridRulerGroup = document.createElementNS(SVG_NS, "g");
    this.gridRulerGroup.className.baseVal = "grid-ruler";
    this.svg.appendChild(this.gridRulerGroup);
    this._renderGridRuler();
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
    if (e.button !== 0) {
      return;
    }
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

  _handleMouseUp(_e) {
    if (this.isDragging && !this.hasMoved && this.selectedElement) {
      this._handleElementClick(this.selectedElement);
    }

    if (this.isDragging && this.hasMoved && this.selectedElement && this.snapToGrid) {
      this._snapElementToGrid(this.selectedElement);
    }

    // 拖拽落定后通知上层（可视化层据此自动保存坐标，实现页面直接编辑位置）
    if (
      this.isDragging &&
      this.hasMoved &&
      this.selectedElement &&
      this.callbacks.onPositionChanged
    ) {
      this.callbacks.onPositionChanged(this.selectedElement.dataset.id);
    }

    this._clearAlignmentLines();
    this.isDragging = false;
  }

  /** 机柜元素位移后，其内部机位子元素按相对坐标跟随（基类联动钩子）。 */
  _onElementPositioned(element, x, y) {
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

  // 从元素 class 推断节点类型（工位/机柜/机位），未知返回空串
  _elementTypeOf(element) {
    const typeByClass = [
      ["workstation-element", "workstation"],
      ["cabinet-element", "cabinet"],
      ["cabinet-position-element", "cabinet-position"]
    ];
    for (const [cls, type] of typeByClass) {
      if (element.classList.contains(cls)) {
        return type;
      }
    }
    return "";
  }

  _handleElementClick(element) {
    const id = element.dataset.id;
    const elementType = this._elementTypeOf(element);

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

  _updateAlignmentLines(element) {
    this._clearAlignmentLines();

    const rect = element.querySelector("rect");
    if (!rect) {
      return;
    }

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
      if (!otherRect) {
        return;
      }

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
    const line = document.createElementNS(SVG_NS, "line");
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
        if (this._cabinetScrollRaf) {
          return;
        }
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

  /** 按横向滚动偏移更新机柜画布 viewBox（背景矩形随 setViewBox 一并铺满）。 */
  _applyCabinetScroll(scrollLeft) {
    const viewWidth = this.container.clientWidth || 800;
    this.setViewBox(scrollLeft, 0, viewWidth, this._cabinetViewHeight || 600);
  }
}
