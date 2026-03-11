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
    this.snapThreshold = 15;
    this.alignmentLines = [];
    this.tooltip = null;
    this.alignmentLinesGroup = null;
    this.snapIndicatorGroup = null;
    this.callbacks = callbacks;
    this.apiGet = apiGet;
    this.apiPost = apiPost;
    this.apiDelete = apiDelete;
    this.showToast = showToast;
    this._rafId = null;
    this._pendingMove = null;
    this._elementPositionsCache = new Map();
    this._cacheValid = false;

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

    this.snapIndicatorGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.snapIndicatorGroup.className.baseVal = "snap-indicators";
    this.svg.appendChild(this.snapIndicatorGroup);

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
      this._clearSnapIndicators();
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
      this._pendingMove = e;
      if (!this._rafId) {
        this._rafId = requestAnimationFrame(() => this._processMove());
      }
      return;
    }

    this._updateTooltip(e);
  }

  _processMove() {
    this._rafId = null;
    
    if (!this._pendingMove || !this.selectedElement) return;
    
    const e = this._pendingMove;
    this._pendingMove = null;

    const currentPos = this._getSvgCoordinates(e);
    let dx = currentPos.x - this.dragStart.x;
    let dy = currentPos.y - this.dragStart.y;

    if (Math.abs(dx) > 2 || Math.abs(dy) > 2) {
      this.hasMoved = true;
      
      if (this.snapToGrid) {
        const snapResult = this._calculateSnapOffsetFast(dx, dy);
        dx = snapResult.dx;
        dy = snapResult.dy;
        this._showSnapIndicatorsFast(snapResult.snapX, snapResult.snapY);
      }
      
      this._updateElementPositionFast(dx, dy);
      this.dragStart = currentPos;
      this._cacheValid = false;

      if (this.showAlignmentLines) {
        this._updateAlignmentLinesThrottled();
      }

      this._hideTooltip();
    }
  }

  _handleMouseUp(e) {
    if (this._rafId) {
      cancelAnimationFrame(this._rafId);
      this._rafId = null;
    }
    this._pendingMove = null;

    if (this.isDragging && !this.hasMoved && this.selectedElement) {
      this._handleElementClick(this.selectedElement);
    }

    if (this.isDragging && this.selectedElement && this.snapToGrid) {
      this._snapElementToGrid(this.selectedElement);
    }

    this._clearAlignmentLines();
    this._clearSnapIndicators();
    this._cacheValid = false;
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

  _updateElementPositionFast(dx, dy) {
    const element = this.selectedElement;
    if (!element) return;

    const transform = element.getAttribute("transform") || "";
    const match = transform.match(/translate\(\s*([-\d.]+)[\s,]+([-\d.]+)\s*\)/);
    let currentTx = match ? parseFloat(match[1]) : 0;
    let currentTy = match ? parseFloat(match[2]) : 0;

    currentTx += dx;
    currentTy += dy;

    element.setAttribute("transform", `translate(${currentTx}, ${currentTy})`);
  }

  _calculateSnapOffsetFast(dx, dy) {
    const element = this.selectedElement;
    if (!element) return { dx, dy, snapX: false, snapY: false };

    const rect = element.querySelector("rect");
    if (!rect) return { dx, dy, snapX: false, snapY: false };

    const baseX = parseFloat(rect.getAttribute("x")) || 0;
    const baseY = parseFloat(rect.getAttribute("y")) || 0;
    const width = parseFloat(rect.getAttribute("width")) || 160;
    const height = parseFloat(rect.getAttribute("height")) || 160;

    const transform = element.getAttribute("transform") || "";
    const match = transform.match(/translate\(\s*([-\d.]+)[\s,]+([-\d.]+)\s*\)/);
    const currentTx = match ? parseFloat(match[1]) : 0;
    const currentTy = match ? parseFloat(match[2]) : 0;

    const newX = baseX + currentTx + dx;
    const newY = baseY + currentTy + dy;

    let finalDx = dx;
    let finalDy = dy;
    let snapX = false;
    let snapY = false;

    const gridSnappedX = Math.round(newX / this.gridSize) * this.gridSize;
    const gridSnappedY = Math.round(newY / this.gridSize) * this.gridSize;

    if (Math.abs(newX - gridSnappedX) <= this.snapThreshold) {
      finalDx = gridSnappedX - baseX - currentTx;
      snapX = true;
    }

    if (Math.abs(newY - gridSnappedY) <= this.snapThreshold) {
      finalDy = gridSnappedY - baseY - currentTy;
      snapY = true;
    }

    return { dx: finalDx, dy: finalDy, snapX, snapY };
  }

  _showSnapIndicatorsFast(snapX, snapY) {
    this._clearSnapIndicators();

    if (!snapX && !snapY) return;

    const element = this.selectedElement;
    if (!element) return;

    const rect = element.querySelector("rect");
    if (!rect) return;

    const baseX = parseFloat(rect.getAttribute("x")) || 0;
    const baseY = parseFloat(rect.getAttribute("y")) || 0;
    const width = parseFloat(rect.getAttribute("width")) || 160;
    const height = parseFloat(rect.getAttribute("height")) || 160;

    const transform = element.getAttribute("transform") || "";
    const match = transform.match(/translate\(\s*([-\d.]+)[\s,]+([-\d.]+)\s*\)/);
    const currentTx = match ? parseFloat(match[1]) : 0;
    const currentTy = match ? parseFloat(match[2]) : 0;

    const x = baseX + currentTx;
    const y = baseY + currentTy;

    if (snapX) {
      const snappedX = Math.round(x / this.gridSize) * this.gridSize;
      this._drawSnapLine(snappedX, y - 20, snappedX, y + height + 20, "vertical");
    }

    if (snapY) {
      const snappedY = Math.round(y / this.gridSize) * this.gridSize;
      this._drawSnapLine(x - 20, snappedY, x + width + 20, snappedY, "horizontal");
    }
  }

  _updateAlignmentLinesThrottled() {
    this._clearAlignmentLines();
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

    const baseX = parseFloat(rect.getAttribute("x")) || 0;
    const baseY = parseFloat(rect.getAttribute("y")) || 0;

    const transform = element.getAttribute("transform") || "";
    const match = transform.match(/translate\(\s*([-\d.]+)[\s,]+([-\d.]+)\s*\)/);
    const currentTx = match ? parseFloat(match[1]) : 0;
    const currentTy = match ? parseFloat(match[2]) : 0;

    const finalX = baseX + currentTx;
    const finalY = baseY + currentTy;

    const snappedX = Math.round(finalX / grid) * grid;
    const snappedY = Math.round(finalY / grid) * grid;

    this._applyFinalPosition(element, snappedX, snappedY);
  }

  _applyFinalPosition(element, finalX, finalY) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const currentX = parseFloat(rect.getAttribute("x")) || 0;
    const currentY = parseFloat(rect.getAttribute("y")) || 0;

    const dx = finalX - currentX;
    const dy = finalY - currentY;

    if (dx !== 0 || dy !== 0) {
      this._updateElementPosition(element, dx, dy);
    }

    element.removeAttribute("transform");
  }

  _calculateSnapOffset(element, dx, dy) {
    const rect = element.querySelector("rect");
    if (!rect) return { dx, dy, snapX: false, snapY: false };

    const currentX = parseFloat(rect.getAttribute("x"));
    const currentY = parseFloat(rect.getAttribute("y"));
    const width = parseFloat(rect.getAttribute("width"));
    const height = parseFloat(rect.getAttribute("height"));
    const newX = currentX + dx;
    const newY = currentY + dy;

    let finalDx = dx;
    let finalDy = dy;
    let snapX = false;
    let snapY = false;

    const gridSnappedX = Math.round(newX / this.gridSize) * this.gridSize;
    const gridSnappedY = Math.round(newY / this.gridSize) * this.gridSize;

    const elementSnap = this._findElementSnapPositions(element, newX, newY, width, height);

    let targetX = newX;
    let xDistance = Infinity;

    if (Math.abs(newX - gridSnappedX) <= this.snapThreshold) {
      xDistance = Math.abs(newX - gridSnappedX);
      targetX = gridSnappedX;
      snapX = true;
    }

    elementSnap.xSnaps.forEach(snap => {
      if (snap.distance <= this.snapThreshold && snap.distance < xDistance) {
        xDistance = snap.distance;
        targetX = snap.position;
        snapX = true;
      }
    });

    let targetY = newY;
    let yDistance = Infinity;

    if (Math.abs(newY - gridSnappedY) <= this.snapThreshold) {
      yDistance = Math.abs(newY - gridSnappedY);
      targetY = gridSnappedY;
      snapY = true;
    }

    elementSnap.ySnaps.forEach(snap => {
      if (snap.distance <= this.snapThreshold && snap.distance < yDistance) {
        yDistance = snap.distance;
        targetY = snap.position;
        snapY = true;
      }
    });

    if (snapX) {
      finalDx = targetX - currentX;
    }
    if (snapY) {
      finalDy = targetY - currentY;
    }

    return { dx: finalDx, dy: finalDy, snapX, snapY };
  }

  _findElementSnapPositions(element, newX, newY, width, height) {
    const xSnaps = [];
    const ySnaps = [];

    const allElements = this.elementsGroup.querySelectorAll("[data-id]");
    const otherElements = Array.from(allElements).filter(el => el !== element);

    const elementCenterX = newX + width / 2;
    const elementCenterY = newY + height / 2;
    const elementRight = newX + width;
    const elementBottom = newY + height;

    otherElements.forEach(other => {
      const otherRect = other.querySelector("rect");
      if (!otherRect) return;

      const ox = parseFloat(otherRect.getAttribute("x"));
      const oy = parseFloat(otherRect.getAttribute("y"));
      const ow = parseFloat(otherRect.getAttribute("width"));
      const oh = parseFloat(otherRect.getAttribute("height"));
      const otherCenterX = ox + ow / 2;
      const otherCenterY = oy + oh / 2;
      const otherRight = ox + ow;
      const otherBottom = oy + oh;

      xSnaps.push({ position: ox, distance: Math.abs(newX - ox) });
      xSnaps.push({ position: otherRight - width, distance: Math.abs(elementRight - otherRight) });
      xSnaps.push({ position: otherCenterX - width / 2, distance: Math.abs(elementCenterX - otherCenterX) });

      ySnaps.push({ position: oy, distance: Math.abs(newY - oy) });
      ySnaps.push({ position: otherBottom - height, distance: Math.abs(elementBottom - otherBottom) });
      ySnaps.push({ position: otherCenterY - height / 2, distance: Math.abs(elementCenterY - otherCenterY) });
    });

    return { xSnaps, ySnaps };
  }

  _showSnapIndicators(element, snapX, snapY) {
    this._clearSnapIndicators();

    if (!snapX && !snapY) return;

    const rect = element.querySelector("rect");
    if (!rect) return;

    const x = parseFloat(rect.getAttribute("x"));
    const y = parseFloat(rect.getAttribute("y"));
    const width = parseFloat(rect.getAttribute("width"));
    const height = parseFloat(rect.getAttribute("height"));

    if (snapX) {
      const snappedX = Math.round(x / this.gridSize) * this.gridSize;
      this._drawSnapLine(snappedX, y - 20, snappedX, y + height + 20, "vertical");
    }

    if (snapY) {
      const snappedY = Math.round(y / this.gridSize) * this.gridSize;
      this._drawSnapLine(x - 20, snappedY, x + width + 20, snappedY, "horizontal");
    }
  }

  _drawSnapLine(x1, y1, x2, y2, type) {
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", x1);
    line.setAttribute("y1", y1);
    line.setAttribute("x2", x2);
    line.setAttribute("y2", y2);
    line.setAttribute("stroke", "#22c55e");
    line.setAttribute("stroke-width", "2");
    line.setAttribute("stroke-dasharray", "6,3");
    line.setAttribute("opacity", "0.8");
    line.dataset.snapIndicator = "true";
    this.snapIndicatorGroup.appendChild(line);
  }

  _clearSnapIndicators() {
    if (this.snapIndicatorGroup) {
      this.snapIndicatorGroup.innerHTML = "";
    }
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
