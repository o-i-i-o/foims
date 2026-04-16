import type { Cabinet } from "../types/resources.js";
export declare function loadCabinetsData(page?: number, sortBy?: string | null, sortOrder?: string | null): Promise<void>;
export declare function initCabinetSortEvents(): void;
export declare function initCabinetTableEvents(): void;
export declare function editCabinet(id: string | number): Promise<void>;
export declare function deleteCabinet(id: string | number): Promise<void>;
export declare function openCabinetModal(cabinet?: Record<string, unknown> | null): Promise<void>;
export declare function submitCabinetForm(): Promise<boolean | void>;
export declare function loadCabinetsForModalSelect(): Promise<Cabinet[]>;
export declare function cleanup(): void;
//# sourceMappingURL=cabinet.d.ts.map