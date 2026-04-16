import type { SVGCore } from "./SVGCore.js";
interface Workstation {
    id: string;
    name: string;
    manager?: string;
    position?: {
        x: number;
        y: number;
        width: number;
        height: number;
    };
    ipManager?: {
        ip_address?: string;
        status?: string;
        switch_name?: string;
        switch_port_number?: string;
        network_name?: string;
    } | null;
}
interface Cabinet {
    id: string;
    name: string;
    capacity?: number;
    position?: {
        x: number;
        y: number;
        width: number;
        height: number;
    };
}
interface CabinetPosition {
    id: string;
    name: string;
    start_u: number;
    end_u: number;
    ipManager?: {
        ip_address?: string;
        switch_name?: string;
        switch_port_number?: string;
    } | null;
}
export declare class SVGRenderer {
    private elementsGroup;
    constructor(core: SVGCore);
    drawWorkstation(workstation: Workstation): SVGGElement;
    drawCabinet(cabinet: Cabinet): SVGGElement;
    private drawUMarks;
    drawCabinetPosition(position: CabinetPosition, cabinet: Cabinet): SVGGElement;
    drawDoor(): SVGGElement;
    clearElements(): void;
}
export {};
//# sourceMappingURL=SVGRenderer.d.ts.map