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
export declare class SVGCore {
    container: HTMLElement;
    svg: SVGSVGElement;
    elementsGroup: SVGGElement;
    gridGroup: SVGGElement;
    alignmentGroup: SVGGElement;
    type: "workstation" | "cabinet";
    callbacks: SVGCoreCallbacks;
    gridSize: number;
    snapToGrid: boolean;
    showAlignmentLines: boolean;
    selectedElement: SVGElement | null;
    isDragging: boolean;
    dragOffset: {
        x: number;
        y: number;
    };
    currentRoomId: string | null;
    currentNetworkRegionId: string | null;
    apiGet: typeof import("../../utils/apiClient.js").ApiClient.get;
    apiPost: typeof import("../../utils/apiClient.js").ApiClient.post;
    apiDelete: typeof import("../../utils/apiClient.js").ApiClient.delete;
    showToast: typeof showToast;
    constructor(containerId: string, type: "workstation" | "cabinet", callbacks?: SVGCoreCallbacks);
    drawGrid(): void;
    initEvents(): void;
    handleMouseDown(e: MouseEvent): void;
    handleMouseMove(e: MouseEvent): void;
    handleMouseUp(): void;
    handleClick(e: MouseEvent): void;
    handleMouseOver(e: MouseEvent): void;
    handleMouseOut(e: MouseEvent): void;
    showTooltip(e: MouseEvent, text: string): void;
    hideTooltip(): void;
    updateElementPosition(element: SVGElement, x: number, y: number): void;
    getElementPosition(element: SVGElement): Position;
    getElementData(element: SVGElement): Record<string, unknown>;
    drawAlignmentLines(x: number, y: number): void;
    clearAlignmentLines(): void;
    deselectAll(): void;
    setGridSize(size: number): void;
    toggleSnapToGrid(enabled: boolean): void;
    toggleAlignmentLines(enabled: boolean): void;
    destroy(): void;
}
//# sourceMappingURL=SVGCore.d.ts.map