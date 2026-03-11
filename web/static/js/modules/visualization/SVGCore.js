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
    this.dragStart = { x: 0, y: 0 };
    this.currentRoomId = null;
    this.currentNetworkRegionId = null;
    this.currentCabinetId = null;
    this.gridSize = 20;
    this.snapToGrid = true;
    this.showAlignmentLines = true;
    this.alignmentThreshold = 10;
    this.alignmentLines = [];
    this.tooltip = null;
    this.alignmentLinesGroup = null;
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

    const smallGridPattern = document.createElementNS("http://www.w3.org/2000/svg", "pattern");
    const smallGridId = `small-grid-${this.type}`;
    smallGridPattern.setAttribute("id", smallGridId);
    smallGridPattern.setAttribute("width", this.gridSize / 2);
    smallGridPattern.setAttribute("height", this.gridSize / 2);
    smallGridPattern.setAttribute("patternUnits", "userSpaceOnUse");

    const smallGridPath = document.createElementNS("http://www.w3.org/2000/svg", "path");
    smallGridPath.setAttribute("d", `M ${this.gridSize / 2} 0 L 0 0 0 ${this.gridSize / 2}`);
    smallGridPath.setAttribute("fill", "none");
    smallGridPath.setAttribute("stroke", "var(--border-light)");
    smallGridPath.setAttribute("stroke-width", "0.25");
    smallGridPath.setAttribute("opacity", "0.5");

    smallGridPattern.appendChild(smallGridPath);
    defs.appendChild(smallGridPattern);

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
      this.dragStart = this._getSvgCoordinates(e);
      this._selectElement(target);
    }
  }

  _handleMouseMove(e) {
    if (this.isDragging && this.selectedElement) {
      const currentPos = this._getSvgCoordinates(e);
      const dx = currentPos.x - this.dragStart.x;
      const dy = currentPos.y - this.dragStart.y;

      if (Math.abs(dx) > 2 || Math.abs(dy) > 2) {
        this.hasMoved = true;
        this._updateElementPosition(this.selectedElement, dx, dy);
        this.dragStart = currentPos;

        if (this.snapToGrid) {
          this._showSnapPreview(this.selectedElement);
        }

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

    if (this.isDragging && this.selectedElement && this.snapToGrid) {
      this._snapElementToGrid(this.selectedElement);
    }

    this._clearAlignmentLines();
    this.isDragging = false;
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

  _updateElementPosition(element, dx, dy) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const x = parseFloat(rect.getAttribute("x")) + dx;
    const y = parseFloat(rect.getAttribute("y")) + dy;

    rect.setAttribute("x", x);
    rect.setAttribute("y", y);

    const texts = element.querySelectorAll("text");
    texts.forEach((text) => {
      const textX = parseFloat(text.getAttribute("x")) + dx;
      const textY = parseFloat(text.getAttribute("y")) + dy;
      text.setAttribute("x", textX);
      text.setAttribute("y", textY);
    });

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

    const childRects = element.querySelectorAll("rect:not(:first-child)");
    childRects.forEach((childRect) => {
      const childX = parseFloat(childRect.getAttribute("x")) + dx;
      const childY = parseFloat(childRect.getAttribute("y")) + dy;
      childRect.setAttribute("x", childX);
      childRect.setAttribute("y", childY);
    });

    if (element.classList.contains("door-element")) {
      const doorHandle = element.querySelector("circle");
      if (doorHandle) {
        const handleX = parseFloat(doorHandle.getAttribute("cx")) + dx;
        const handleY = parseFloat(doorHandle.getAttribute("cy")) + dy;
        doorHandle.setAttribute("cx", handleX);
        doorHandle.setAttribute("cy", handleY);
      }
    }

    if (element.classList.contains("cabinet-element")) {
      this._updateChildPositions(element, dx, dy);
    }
  }

  _updateChildPositions(element, dx, dy) {
    const cabinetId = element.dataset.id;
    const positions = this.elementsGroup.querySelectorAll(".cabinet-position-element");
    positions.forEach((position) => {
      if (position.dataset.cabinetId === cabinetId) {
        const positionRect = position.querySelector("rect");
        if (positionRect) {
          const newPosX = parseFloat(positionRect.getAttribute("x")) + dx;
          const newPosY = parseFloat(positionRect.getAttribute("y")) + dy;
          positionRect.setAttribute("x", newPosX);
          positionRect.setAttribute("y", newPosY);
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

  _snapElementToGrid(element) {
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
      this._updateElementPosition(element, dx, dy);
    }
  }

  _showSnapPreview(element) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const x = parseFloat(rect.getAttribute("x"));
    const y = parseFloat(rect.getAttribute("y"));
    const snappedX = Math.round(x / this.gridSize) * this.gridSize;
    const snappedY = Math.round(y / this.gridSize) * this.gridSize;

    element.dataset.snapX = snappedX;
    element.dataset.snapY = snappedY;
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

    const alignments = this._calculateAlignments(otherElements, x, y, width, height, elementCenterX, elementCenterY);

    alignments.forEach(alignment => {
      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("stroke", "#3b82f6");
      line.setAttribute("stroke-width", "1");
      line.setAttribute("stroke-dasharray", "4,4");
      line.setAttribute("opacity", "0.7");

      if (alignment.type === 'vertical') {
        line.setAttribute("x1", alignment.x);
        line.setAttribute("y1", alignment.y1);
        line.setAttribute("x2", alignment.x);
        line.setAttribute("y2", alignment.y2);
      } else {
        line.setAttribute("x1", alignment.x1);
        line.setAttribute("y1", alignment.y);
        line.setAttribute("x2", alignment.x2);
        line.setAttribute("y2", alignment.y);
      }

      this.alignmentLinesGroup.appendChild(line);
    });
  }

  _calculateAlignments(otherElements, x, y, width, height, elementCenterX, elementCenterY) {
    const alignments = [];

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
        alignments.push({
          type: 'vertical',
          x: otherCenterX,
          y1: Math.min(y, oy),
          y2: Math.max(y + height, oy + oh)
        });
      }

      if (Math.abs(elementCenterY - otherCenterY) < this.alignmentThreshold) {
        alignments.push({
          type: 'horizontal',
          y: otherCenterY,
          x1: Math.min(x, ox),
          x2: Math.max(x + width, ox + ow)
        });
      }

      if (Math.abs(x - ox) < this.alignmentThreshold) {
        alignments.push({
          type: 'vertical',
          x: ox,
          y1: Math.min(y, oy),
          y2: Math.max(y + height, oy + oh)
        });
      }

      if (Math.abs(x + width - ox - ow) < this.alignmentThreshold) {
        alignments.push({
          type: 'vertical',
          x: ox + ow,
          y1: Math.min(y, oy),
          y2: Math.max(y + height, oy + oh)
        });
      }

      if (Math.abs(y - oy) < this.alignmentThreshold) {
        alignments.push({
          type: 'horizontal',
          y: oy,
          x1: Math.min(x, ox),
          x2: Math.max(x + width, ox + ow)
        });
      }

      if (Math.abs(y + height - oy - oh) < this.alignmentThreshold) {
        alignments.push({
          type: 'horizontal',
          y: oy + oh,
          x1: Math.min(x, ox),
          x2: Math.max(x + width, ox + ow)
        });
      }
    });

    return alignments;
  }

  _clearAlignmentLines() {
    this.alignmentLinesGroup.innerHTML = "";
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
