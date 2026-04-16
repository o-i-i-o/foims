import type { SVGCore, Position } from "./SVGCore.js";
import type { SVGRenderer } from "./SVGRenderer.js";
interface Workstation {
    id: string;
    name: string;
    manager?: string;
    position?: Position;
    ipManager?: IpManager | null;
}
interface IpManager {
    id: string;
    ip_address: string;
    status?: string;
    switch_name?: string;
    switch_port_number?: string;
    network_name?: string;
    workstation_id?: string;
    position_id?: string;
}
interface Cabinet {
    id: string;
    name: string;
    capacity?: number;
    position?: Position;
    networks?: {
        network_region_id: string;
    }[];
}
interface CabinetPosition {
    id: string;
    name: string;
    start_u: number;
    end_u: number;
    ipManager?: IpManager | null;
}
export declare class SVGDataManager {
    private core;
    private renderer;
    constructor(core: SVGCore, renderer: SVGRenderer);
    private apiGet;
    private apiPost;
    private apiDelete;
    private showToast;
    fetchWorkstationsByRoom(roomId: string): Promise<Workstation[]>;
    fetchIpManager(): Promise<IpManager[]>;
    fetchCabinetsByNetworkRegion(networkRegionId: string): Promise<Cabinet[]>;
    fetchCabinetPositions(cabinetId: string): Promise<CabinetPosition[]>;
    loadSavedLayout(id: string): Promise<boolean>;
    drawCabinetPositionsWithIp(cabinet: Cabinet): Promise<void>;
    saveLayout(): Promise<void>;
    deleteLayout(): Promise<void>;
}
export {};
//# sourceMappingURL=SVGDataManager.d.ts.map