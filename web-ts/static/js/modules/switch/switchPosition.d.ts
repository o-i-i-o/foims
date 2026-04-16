interface SwitchData {
    cabinet_id?: string;
    cabinet_name?: string;
    start_u?: number;
    end_u?: number;
    network_region_id?: string;
    network_region_name?: string;
    position?: {
        cabinet_id?: string;
        cabinet_name?: string;
        start_u?: number;
        end_u?: number;
        network_region_id?: string;
    };
}
export declare class PositionSelector {
    private onSelect;
    private eventController;
    private cabinetsCache;
    constructor(onSelect: (regionId: string | null, regionName: string) => void);
    init(sw?: SwitchData | null): Promise<void>;
    private bindEvents;
    loadFromSwitch(sw: SwitchData): Promise<void>;
    clear(): void;
    destroy(): void;
}
export {};
//# sourceMappingURL=switchPosition.d.ts.map