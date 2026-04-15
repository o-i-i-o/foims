import type { Network } from "../types/resources.js";
declare class EventHandler {
    private handlers;
    constructor();
    bind(element: Element, event: string, handler: EventListener): EventListener;
    clear(): void;
}
interface NetworkConfigOptions {
    containerId: string;
    regionSelectClass: string;
    networkSelectClass: string;
    addBtnId: string;
    removeBtnClass: string;
}
declare class NetworkConfigManager {
    options: NetworkConfigOptions;
    container: HTMLElement | null;
    eventHandler: EventHandler;
    constructor(options: NetworkConfigOptions);
    init(): Promise<boolean>;
    createItemHTML(): string;
    addItem(): Promise<Element | undefined>;
    updateAddButtons(): void;
    bindItemEvents(item: Element): void;
    removeItem(item: Element): void;
    onRegionChange(regionSelect: HTMLSelectElement, item: Element): Promise<void>;
    updateNetworkSelects(): Promise<void>;
    getSelectedNetworkIds(): string[];
    loadExistingNetworks(networks: {
        id: number;
    }[], allNetworks: Network[]): Promise<void>;
    collectData(): {
        networkIds: string[];
        hasEmpty: boolean;
    };
    destroy(): void;
}
export declare const roomNetworkConfigManager: NetworkConfigManager;
export declare function loadRoomsData(page?: number, sortBy?: string | null, sortOrder?: string | null): Promise<void>;
export declare function initRoomSortEvents(): void;
export declare function editRoom(id: string | number): Promise<void>;
export declare function deleteRoom(id: string | number): Promise<void>;
export declare function submitRoomForm(): Promise<boolean | void>;
export declare function openRoomModal(room?: Record<string, unknown> | null): Promise<void>;
export {};
//# sourceMappingURL=room.d.ts.map