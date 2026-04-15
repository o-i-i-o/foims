import { SVGCore } from "./SVGCore.js";
import { SVGRenderer } from "./SVGRenderer.js";
import { SVGDataManager } from "./SVGDataManager.js";
export declare class SVGVisualization {
    core: SVGCore;
    renderer: SVGRenderer;
    dataManager: SVGDataManager;
    elementsGroup: SVGGElement;
    svg: SVGSVGElement;
    container: HTMLElement;
    gridSize: number;
    snapToGrid: boolean;
    showAlignmentLines: boolean;
    currentRoomId: string | null;
    currentNetworkRegionId: string | null;
    constructor(containerId: string, type: "workstation" | "cabinet", callbacks?: {});
    autoDrawWorkstations(roomId: string): Promise<void>;
    autoDrawCabinetPositions(networkRegionId: string): Promise<void>;
    loadSavedLayout(id: string): Promise<boolean>;
    saveLayout(): Promise<void>;
    deleteLayout(): Promise<void>;
    deleteWorkstation(id: string): void;
    deleteCabinetPosition(id: string): void;
    setGridSize(size: number): void;
    toggleSnapToGrid(enabled: boolean): void;
    toggleAlignmentLines(enabled: boolean): void;
    destroy(): void;
}
//# sourceMappingURL=SVGVisualization.d.ts.map