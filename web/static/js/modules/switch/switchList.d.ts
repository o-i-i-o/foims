interface Switch {
    id: string;
    name: string;
    ip_address: string;
    vendor: string;
    model: string;
    description?: string;
    status?: string;
    ports_count?: number;
    active_ports_count?: number;
    created_at?: string;
}
export declare function loadSwitchesData(page?: number, searchTerm?: string): Promise<void>;
export declare function fetchSwitchById(id: string): Promise<Switch | null>;
export declare function deleteSwitch(id: string | number): Promise<boolean>;
export declare function submitSwitchForm(): Promise<boolean>;
export {};
//# sourceMappingURL=switchList.d.ts.map