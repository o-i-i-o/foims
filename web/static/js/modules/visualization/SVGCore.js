import { apiGet, apiPost, apiDelete } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";

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
    this.currentNetworkRegionId = null;
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

    this._createDefs();
    this._createGridBackground();

    this.elementsGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.svg.appendChild(this.elementsGroup);

    this.alignmentLinesGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.alignmentLinesGroup.className.baseVal = "alignment-lines";
    this.svg.appendChild(this.alignmentLinesGroup);

    this.container.appendChild(this.svg);

    this.container.style.overflow = "auto";
    this.container.style.maxHeight = "800px";
    this.container.style.border = "1px solid #ddd";
    this.container.style.borderRadius = "4px";
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
    this.svg.appendChild(rect);
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

    if (elementType === "workstation" && this.callbacks.onEditWorkstation) {
      this.callbacks.onEditWorkstation(id);
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
    const otherElements = Array.from(allElements).filter(el => el !== element);

    otherElements.forEach(other => {
      const otherRect = other.querySelector("rect");
      if (!otherRect) return;

      const ox = parseFloat(otherRect.getAttribute("x"));
      const oy = parseFloat(otherRect.getAttribute("y"));
      const ow = parseFloat(otherRect.getAttribute("width"));
      const oh = parseFloat(otherRect.getAttribute("height"));
      const otherCenterX = ox + ow / 2;
      const otherCenterY = oy + oh / 2;

      if (Math.abs(elementCenterX - otherCenterX) < this.alignmentThreshold) {
        this._drawAlignmentLine(otherCenterX, Math.min(y, oy), otherCenterX, Math.max(y + height, oy + oh));
      }

      if (Math.abs(elementCenterY - otherCenterY) < this.alignmentThreshold) {
        this._drawAlignmentLine(Math.min(x, ox), otherCenterY, Math.max(x + width, ox + ow), otherCenterY);
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
    this.tooltip.style.left = `${e.clientX + 12}px`;
    this.tooltip.style.top = `${e.clientY + 12}px`;
    this.tooltip.classList.add("visible");
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
