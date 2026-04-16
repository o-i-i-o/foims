interface DashboardStats {
    networks?: {
        networks?: number;
        regions?: number;
    };
    ips?: {
        total?: number;
        active?: number;
        by_device_type?: Record<string, number>;
        by_status?: Record<string, number>;
    };
    activity?: {
        operations_24h?: number;
        logins_24h?: number;
    };
    users?: {
        total?: number;
        active?: number;
    };
    locations?: {
        rooms?: number;
        cabinets?: number;
        workstations?: number;
        positions?: number;
    };
    switches?: number;
    rooms_by_type?: Record<string, number>;
}
export declare function loadDashboardData(forceRefresh?: boolean): Promise<void>;
export declare function getStatsCache(): DashboardStats | null;
export declare function clearDashboardCache(): void;
export {};
//# sourceMappingURL=dashboard.d.ts.map