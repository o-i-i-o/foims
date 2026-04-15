export declare function editWorkstation(id: string | number): Promise<void>;
export declare function deleteWorkstation(id: string | number): Promise<void>;
export declare function loadWorkstationsData(page?: number, sortBy?: string | null, sortOrder?: string | null): Promise<void>;
export declare function initWorkstationSortEvents(): void;
export declare function openWorkstationModal(workstation?: Record<string, unknown> | null): Promise<void>;
export declare function submitWorkstationForm(): Promise<void>;
//# sourceMappingURL=workstation.d.ts.map