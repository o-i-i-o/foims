export declare function loadNetworkTypesData(page?: number): Promise<void>;
export declare function loadNetworksData(page?: number, searchTerm?: string): Promise<void>;
export declare function initNetworksSearch(): void;
export declare function calculateTotalIps(cidr: string | null | undefined): number;
export declare function generateIpAddresses(cidr: string | null | undefined): string[];
export declare function showNetworkUsage(id: string | number): Promise<void>;
export declare function editNetwork(id: string | number): Promise<void>;
export declare function deleteNetwork(id: string | number): Promise<void>;
export declare function editNetworkType(id: string | number): Promise<void>;
export declare function deleteNetworkType(id: string | number): Promise<void>;
export declare function submitNetworkTypeForm(): Promise<boolean | void>;
export declare function submitNetworkForm(): Promise<boolean | void>;
export declare function openNetworkTypeModal(networkType?: Record<string, unknown> | null): void;
export declare function openNetworkModal(network?: Record<string, unknown> | null): Promise<void>;
//# sourceMappingURL=networks.d.ts.map