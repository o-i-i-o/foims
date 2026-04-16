// SVG Core - Base functionality for SVG visualization
import { apiGet, apiPost, apiDelete } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";

export interface SVGCoreCallbacks {
  onElementSelect?: (element: SVGElement, data: Record<string, unknown>) => void;
  onElementMove?: (element: SVGElement, position: Position) => void;
  onElementDelete?: (id: string) => void;
}

export interface Position {
  x: number;
  y: number;
  width: number;
  height: number;
  rotation?: number;
}

export class SVGCore {
  container: HTMLElement;
  svg: SVGSVGElement;
  elementsGroup: SVGGElement;
  gridGroup: SVGGElement;
  alignmentGroup: SVGGElement;
  type: "workstation" | "cabinet";
  callbacks: SVGCoreCallbacks;

  gridSize = 20;
  snapToGrid = true;
  showAlignmentLines = true;
  selectedElement: SVGElement | null = null;
  isDragging = false;
  dragOffset = { x: 0, y: 0 };
  currentRoomId: string | null = null;
  currentNetworkRegionId: string | null = null;

  // Expose API methods for data manager
  apiGet = apiGet;
  apiPost = apiPost;
  apiDelete = apiDelete;
  showToast = showToast;

  constructor(containerId: string, type: "workstation" | "cabinet", callbacks: SVGCoreCallbacks = {}) {
    this.container = document.getElementById(containerId)!;
    if (!this.container) {
      throw new Error(`Container #${containerId} not found`);
    }

    this.type = type;
    this.callbacks = callbacks;

    this.svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    this.svg.setAttribute("width", "100%");
    this.svg.setAttribute("height", "100%");
    this.svg.setAttribute("viewBox", "0 0 1000 800");
    this.svg.style.cursor = "default";

    this.gridGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.gridGroup.className.baseVal = "grid-group";
    this.svg.appendChild(this.gridGroup);

    this.alignmentGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.alignmentGroup.className.baseVal = "alignment-group";
    this.svg.appendChild(this.alignmentGroup);

    this.elementsGroup = document.createElementNS("http://www.w3.org/2000/svg", "g");
    this.elementsGroup.className.baseVal = "elements-group";
    this.svg.appendChild(this.elementsGroup);

    this.container.appendChild(this.svg);

    this.drawGrid();
    this.initEvents();
  }

  drawGrid(): void {
    this.gridGroup.innerHTML = "";

    const width = 2000;
    const height = 2000;

    for (let x = 0; x <= width; x += this.gridSize) {
      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", String(x));
      line.setAttribute("y1", "0");
      line.setAttribute("x2", String(x));
      line.setAttribute("y2", String(height));
      line.setAttribute("stroke", "#e0e0e0");
      line.setAttribute("stroke-width", "0.5");
      this.gridGroup.appendChild(line);
    }

    for (let y = 0; y <= height; y += this.gridSize) {
      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", "0");
      line.setAttribute("y1", String(y));
      line.setAttribute("x2", String(width));
      line.setAttribute("y2", String(y));
      line.setAttribute("stroke", "#e0e0e0");
      line.setAttribute("stroke-width", "0.5");
      this.gridGroup.appendChild(line);
    }
  }

  initEvents(): void {
    this.svg.addEventListener("mousedown", this.handleMouseDown.bind(this));
    this.svg.addEventListener("mousemove", this.handleMouseMove.bind(this));
    this.svg.addEventListener("mouseup", this.handleMouseUp.bind(this));
    this.svg.addEventListener("click", this.handleClick.bind(this));

    // Tooltip events
    this.svg.addEventListener("mouseover", this.handleMouseOver.bind(this));
    this.svg.addEventListener("mouseout", this.handleMouseOut.bind(this));
  }

  handleMouseDown(e: MouseEvent): void {
    const target = e.target as SVGElement;
    const group = target.closest("[data-id]") as SVGElement | null;

    if (group && group !== this.elementsGroup) {
      this.selectedElement = group;
      this.isDragging = true;

      const rect = group.querySelector("rect");
      if (rect) {
        const x = parseFloat(rect.getAttribute("x") || "0");
        const y = parseFloat(rect.getAttribute("y") || "0");
        this.dragOffset = {
          x: e.offsetX - x,
          y: e.offsetY - y,
        };
      }

      group.classList.add("selected");
    }
  }

  handleMouseMove(e: MouseEvent): void {
    if (!this.isDragging || !this.selectedElement) return;

    let x = e.offsetX - this.dragOffset.x;
    let y = e.offsetY - this.dragOffset.y;

    if (this.snapToGrid) {
      x = Math.round(x / this.gridSize) * this.gridSize;
      y = Math.round(y / this.gridSize) * this.gridSize;
    }

    this.updateElementPosition(this.selectedElement, x, y);

    if (this.showAlignmentLines) {
      this.drawAlignmentLines(x, y);
    }
  }

  handleMouseUp(): void {
    if (this.isDragging && this.selectedElement) {
      this.isDragging = false;
      this.clearAlignmentLines();

      if (this.callbacks.onElementMove) {
        const position = this.getElementPosition(this.selectedElement);
        this.callbacks.onElementMove(this.selectedElement, position);
      }
    }
  }

  handleClick(e: MouseEvent): void {
    const target = e.target as SVGElement;

    if (target === this.svg || target === this.gridGroup) {
      this.deselectAll();
      return;
    }

    const group = target.closest("[data-id]") as SVGElement | null;
    if (group && this.callbacks.onElementSelect) {
      const data = this.getElementData(group);
      this.callbacks.onElementSelect(group, data);
    }
  }

  handleMouseOver(e: MouseEvent): void {
    const target = e.target as SVGElement;
    const group = target.closest("[data-id]") as SVGElement | null;

    if (group && group.dataset.tooltip) {
      this.showTooltip(e, group.dataset.tooltip);
    }
  }

  handleMouseOut(e: MouseEvent): void {
    const target = e.target as SVGElement;
    const relatedTarget = e.relatedTarget as HTMLElement;

    if (target.closest("[data-id]") && !relatedTarget?.closest(".tooltip")) {
      this.hideTooltip();
    }
  }

  showTooltip(e: MouseEvent, text: string): void {
    this.hideTooltip();

    const tooltip = document.createElement("div");
    tooltip.className = "tooltip";
    tooltip.textContent = text;
    tooltip.style.cssText = `
      position: fixed;
      background: rgba(0, 0, 0, 0.8);
      color: white;
      padding: 8px 12px;
      border-radius: 4px;
      font-size: 12px;
      pointer-events: none;
      z-index: 1000;
      white-space: pre-line;
    `;

    tooltip.style.left = `${e.clientX + 10}px`;
    tooltip.style.top = `${e.clientY + 10}px`;

    document.body.appendChild(tooltip);
    this.container.dataset.activeTooltip = "true";
  }

  hideTooltip(): void {
    const tooltip = document.querySelector(".tooltip");
    if (tooltip) {
      tooltip.remove();
    }
    delete this.container.dataset.activeTooltip;
  }

  updateElementPosition(element: SVGElement, x: number, y: number): void {
    const rect = element.querySelector("rect");
    if (!rect) return;

    const oldX = parseFloat(rect.getAttribute("x") || "0");
    const oldY = parseFloat(rect.getAttribute("y") || "0");
    const dx = x - oldX;
    const dy = y - oldY;

    rect.setAttribute("x", String(x));
    rect.setAttribute("y", String(y));

    // Update child elements
    const children = element.querySelectorAll("*");
    children.forEach((child) => {
      const el = child as SVGElement;

      if (el.hasAttribute("x") && !el.dataset.relX) {
        const cx = parseFloat(el.getAttribute("x") || "0");
        el.setAttribute("x", String(cx + dx));
      }
      if (el.hasAttribute("y") && !el.dataset.relY) {
        const cy = parseFloat(el.getAttribute("y") || "0");
        el.setAttribute("y", String(cy + dy));
      }
      if (el.hasAttribute("x1")) {
        const x1 = parseFloat(el.getAttribute("x1") || "0");
        el.setAttribute("x1", String(x1 + dx));
      }
      if (el.hasAttribute("y1")) {
        const y1 = parseFloat(el.getAttribute("y1") || "0");
        el.setAttribute("y1", String(y1 + dy));
      }
      if (el.hasAttribute("x2")) {
        const x2 = parseFloat(el.getAttribute("x2") || "0");
        el.setAttribute("x2", String(x2 + dx));
      }
      if (el.hasAttribute("y2")) {
        const y2 = parseFloat(el.getAttribute("y2") || "0");
        el.setAttribute("y2", String(y2 + dy));
      }
      if (el.hasAttribute("cx")) {
        const cx = parseFloat(el.getAttribute("cx") || "0");
        el.setAttribute("cx", String(cx + dx));
      }
      if (el.hasAttribute("cy")) {
        const cy = parseFloat(el.getAttribute("cy") || "0");
        el.setAttribute("cy", String(cy + dy));
      }
    });

    // Update relative positioned elements
    const relElements = element.querySelectorAll("[data-rel-x], [data-rel-y]");
    relElements.forEach((child) => {
      const el = child as SVGElement;

      if (el.dataset.relX) {
        el.setAttribute("x", String(x + parseFloat(el.dataset.relX)));
      }
      if (el.dataset.relY) {
        el.setAttribute("y", String(y + parseFloat(el.dataset.relY)));
      }
      if (el.dataset.relCx) {
        el.setAttribute("cx", String(x + parseFloat(el.dataset.relCx)));
      }
      if (el.dataset.relCy) {
        el.setAttribute("cy", String(y + parseFloat(el.dataset.relCy)));
      }
    });
  }

  getElementPosition(element: SVGElement): Position {
    const rect = element.querySelector("rect");
    if (!rect) {
      return { x: 0, y: 0, width: 0, height: 0 };
    }

    return {
      x: parseFloat(rect.getAttribute("x") || "0"),
      y: parseFloat(rect.getAttribute("y") || "0"),
      width: parseFloat(rect.getAttribute("width") || "0"),
      height: parseFloat(rect.getAttribute("height") || "0"),
    };
  }

  getElementData(element: SVGElement): Record<string, unknown> {
    return {
      id: element.dataset.id,
      type: this.type,
    };
  }

  drawAlignmentLines(x: number, y: number): void {
    this.clearAlignmentLines();

    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", "0");
    line.setAttribute("y1", String(y));
    line.setAttribute("x2", "2000");
    line.setAttribute("y2", String(y));
    line.setAttribute("stroke", "#3b82f6");
    line.setAttribute("stroke-width", "1");
    line.setAttribute("stroke-dasharray", "5,5");
    this.alignmentGroup.appendChild(line);

    const line2 = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line2.setAttribute("x1", String(x));
    line2.setAttribute("y1", "0");
    line2.setAttribute("x2", String(x));
    line2.setAttribute("y2", "2000");
    line2.setAttribute("stroke", "#3b82f6");
    line2.setAttribute("stroke-width", "1");
    line2.setAttribute("stroke-dasharray", "5,5");
    this.alignmentGroup.appendChild(line2);
  }

  clearAlignmentLines(): void {
    this.alignmentGroup.innerHTML = "";
  }

  deselectAll(): void {
    this.selectedElement = null;
    this.isDragging = false;

    const selected = this.elementsGroup.querySelectorAll(".selected");
    selected.forEach((el) => el.classList.remove("selected"));
  }

  setGridSize(size: number): void {
    this.gridSize = size;
    this.drawGrid();
  }

  toggleSnapToGrid(enabled: boolean): void {
    this.snapToGrid = enabled;
  }

  toggleAlignmentLines(enabled: boolean): void {
    this.showAlignmentLines = enabled;
    if (!enabled) {
      this.clearAlignmentLines();
    }
  }

  destroy(): void {
    this.hideTooltip();
    this.container.removeChild(this.svg);
  }
}
