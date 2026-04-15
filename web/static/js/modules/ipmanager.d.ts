export declare function loadSwitchesForPullMac(): Promise<void>;
export declare function loadNetworksForPullMac(): Promise<void>;
export declare function pullIpMacData(): Promise<void>;
interface IpSearchParams {
    search?: string;
    device_type?: string;
    status?: string;
    page?: number;
    page_size?: number;
}
export declare function loadIpMacData(searchParams?: IpSearchParams): Promise<{
    total: number;
    page: number;
    total_pages: number;
} | null>;
export declare const initIpMacFunctions: () => void;
export {};
//# sourceMappingURL=ipmanager.d.ts.map