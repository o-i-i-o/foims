const SVG_NS = "http://www.w3.org/2000/svg";

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
    this.isSpaceDown = false;
    this.isConnectionMode = false;
    this.connectionSource = null;
    this.gridSize = 20;
    this.callbacks = callbacks;

    this._init();
  }

  _init() {
    this._initSVG();
    this._initEventListeners();
  }

  _initSVG() {
    this.container.innerHTML = "";

    this.svg = document.createElementNS(SVG_NS, "svg");
    this.svg.className.baseVal = "visualization-svg";
    this.svg.setAttribute("width", "100%");
    this.svg.setAttribute("height", "100%");
    this.svg.setAttribute("viewBox", "0 0 3000 2000");

    this._createDefs();

    const bg = document.createElementNS(SVG_NS, "rect");
    bg.setAttribute("width", "100%");
    bg.setAttribute("height", "100%");
    bg.setAttribute("fill", "url(#topology-grid)");
    this.svg.appendChild(bg);

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

    this.container.style.overflow = "hidden";
    this.container.style.height = "calc(100vh - 200px)";
    this.container.style.minHeight = "500px";
    this.container.style.border = "1px solid #ddd";
    this.container.style.borderRadius = "4px";
    this.container.style.backgroundColor = "#fafbfc";
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

  _initEventListeners() {
    this.svg.addEventListener("mousedown", this._handleMouseDown.bind(this));
    this.svg.addEventListener("mousemove", this._handleMouseMove.bind(this));
    this.svg.addEventListener("mouseup", this._handleMouseUp.bind(this));
    this.svg.addEventListener("wheel", this._handleWheel.bind(this), { passive: false });
    this.svg.addEventListener("mouseleave", () => {
      this._handleMouseUp();
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

    if (e.button !== 0) return;

    const anchor = e.target.closest(".port-anchor");
    if (anchor) return;

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
          y: parseFloat(rect.getAttribute("y")),
        };
      }
      this.mouseStartPos = this._getSvgCoordinates(e);
    } else {
      this._clearSelection();
      if (this.callbacks.onCanvasClick) {
        this.callbacks.onCanvasClick();
      }
    }
  }

  _handleMouseMove(e) {
    if (this.isPanning) {
      const vb = this.svg.viewBox.baseVal;
      const scale = vb.width / this.container.clientWidth;
      const dx = (e.clientX - this.panStart.x) * scale;
      const dy = (e.clientY - this.panStart.y) * scale;
      this.svg.setAttribute("viewBox", `${this.viewBoxStart.x - dx} ${this.viewBoxStart.y - dy} ${vb.width} ${vb.height}`);
      return;
    }

    if (this.isDragging && this.selectedElement) {
      const currentPos = this._getSvgCoordinates(e);
      const dx = currentPos.x - this.mouseStartPos.x;
      const dy = currentPos.y - this.mouseStartPos.y;
      if (Math.abs(dx) > 1 || Math.abs(dy) > 1) {
        this.hasMoved = true;
        const newX = this.elementStartPos.x + dx;
        const newY = this.elementStartPos.y + dy;
        this._setElementPosition(this.selectedElement, newX, newY);
        if (this.callbacks.onNodeDrag) {
          this.callbacks.onNodeDrag(this.selectedElement.dataset.deviceId);
        }
      }
      return;
    }

    if (this.isConnectionMode && this.connectionSource) {
      const svgPos = this._getSvgCoordinates(e);
      this._drawTempConnection(
        this.connectionSource.x, this.connectionSource.y,
        svgPos.x, svgPos.y
      );
    }
  }

  _handleMouseUp() {
    if (this.isPanning) {
      this.isPanning = false;
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
    const delta = e.deltaY > 0 ? 1.1 : 0.9;
    const vb = this.svg.viewBox.baseVal;
    const mousePos = this._getSvgCoordinates(e);

    const newWidth = vb.width * delta;
    const newHeight = vb.height * delta;

    if (newWidth < 300 || newWidth > 30000) return;

    const newX = mousePos.x - (mousePos.x - vb.x) * delta;
    const newY = mousePos.y - (mousePos.y - vb.y) * delta;

    this.svg.setAttribute("viewBox", `${newX} ${newY} ${newWidth} ${newHeight}`);
    this._updateZoomIndicator();
  }

  _handleAnchorClick(anchor, e) {
    e.stopPropagation();
    const deviceId = anchor.closest("[data-device-id]").dataset.deviceId;
    const portDir = anchor.dataset.portDir;
    const portId = anchor.dataset.portId || null;

    const svgPos = this._getSvgCoordinates(e);
    const anchorPos = {
      x: parseFloat(anchor.getAttribute("cx")),
      y: parseFloat(anchor.getAttribute("cy")),
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
    this.elementsGroup.querySelectorAll(".connection-source").forEach((a) => a.classList.remove("connection-source"));
  }

  _drawTempConnection(x1, y1, x2, y2) {
    this.tempConnectionGroup.innerHTML = "";
    const line = document.createElementNS(SVG_NS, "path");
    const dx = x2 - x1;
    const dy = y2 - y1;
    const dist = Math.sqrt(dx * dx + dy * dy);
    const offset = Math.min(dist * 0.3, 80);
    line.setAttribute("d", `M ${x1} ${y1} C ${x1 + offset} ${y1}, ${x2 - offset} ${y2}, ${x2} ${y2}`);
    line.setAttribute("class", "temp-connection");
    this.tempConnectionGroup.appendChild(line);
  }

  _setElementPosition(element, x, y) {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const width = parseFloat(rect.getAttribute("width")) || 200;
    const height = parseFloat(rect.getAttribute("height")) || 100;

    rect.setAttribute("x", x);
    rect.setAttribute("y", y);

    element.querySelectorAll("text").forEach((text) => {
      const relX = parseFloat(text.dataset.relX || 0);
      const relY = parseFloat(text.dataset.relY || 0);
      text.setAttribute("x", x + width / 2 + relX);
      text.setAttribute("y", y + relY);
    });

    element.querySelectorAll(".port-anchor").forEach((anchor) => {
      const relCx = parseFloat(anchor.dataset.relCx || 0);
      const relCy = parseFloat(anchor.dataset.relCy || 0);
      anchor.setAttribute("cx", x + relCx);
      anchor.setAttribute("cy", y + relCy);
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

  _selectElement(element) {
    this._clearSelection();
    this.selectedElement = element;
    element.classList.add("selected");
  }

  _clearSelection() {
    this.elementsGroup.querySelectorAll(".selected").forEach((el) => el.classList.remove("selected"));
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

  _updateZoomIndicator() {
    const indicator = document.getElementById("topology-zoom-level");
    if (!indicator) return;
    const vb = this.svg.viewBox.baseVal;
    const containerWidth = this.container.clientWidth || 1;
    const scale = containerWidth / vb.width;
    indicator.textContent = Math.round(scale * 100) + "%";
  }

  setConnectionMode(enabled) {
    this.isConnectionMode = enabled;
    if (!enabled) {
      this._cancelConnection();
    }
    this.container.classList.toggle("connection-mode", enabled);
  }

  getNodePosition(deviceId) {
    const element = this.elementsGroup.querySelector(`[data-device-id="${deviceId}"]`);
    if (!element) return null;
    const rect = element.querySelector("rect");
    if (!rect) return null;
    return {
      x: parseFloat(rect.getAttribute("x")),
      y: parseFloat(rect.getAttribute("y")),
      width: parseFloat(rect.getAttribute("width")),
      height: parseFloat(rect.getAttribute("height")),
    };
  }

  getAnchorPosition(deviceId, direction) {
    const pos = this.getNodePosition(deviceId);
    if (!pos) return null;
    switch (direction) {
      case "top": return { x: pos.x + pos.width / 2, y: pos.y };
      case "right": return { x: pos.x + pos.width, y: pos.y + pos.height / 2 };
      case "bottom": return { x: pos.x + pos.width / 2, y: pos.y + pos.height };
      case "left": return { x: pos.x, y: pos.y + pos.height / 2 };
      default: return { x: pos.x + pos.width / 2, y: pos.y + pos.height / 2 };
    }
  }

  resetZoom() {
    this.svg.setAttribute("viewBox", "0 0 3000 2000");
    this._updateZoomIndicator();
  }
}
