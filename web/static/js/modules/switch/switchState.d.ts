export declare const SWITCH_PAGE_SIZE = 20;
export declare const SWITCH_PORT_PAGE_SIZE = 20;
interface SwitchListState {
    currentSwitchId: string | null;
    currentSwitchName: string;
    currentSwitchPage: number;
    networkRegionId: string | null;
    networkRegionName: string;
}
export declare const listState: SwitchListState;
interface PositionData {
    cabinetId: string | null;
    cabinetName: string | null;
    startU: number | null;
    endU: number | null;
    positionId: string | null;
    networkRegionId: string | null;
}
export declare const positionData: PositionData;
interface NetworkRegionState {
    id: string | null;
    name: string;
}
export declare const networkRegion: NetworkRegionState;
export declare function updateNetworkRegion(regionId: string | null, regionName: string): void;
export declare function setCurrentSwitchId(id: string | null): void;
export declare function setCurrentSwitchName(name: string): void;
export declare function setCurrentSwitchPage(page: number): void;
export declare function getCurrentSwitchId(): string | null;
export declare function getCurrentSwitchName(): string;
export {};
//# sourceMappingURL=switchState.d.ts.map