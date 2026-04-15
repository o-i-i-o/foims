interface LogSearchParams {
    resource_type?: string;
    resource_id?: string;
    user_id?: string;
    action?: string;
    page?: number;
    page_size?: number;
}
export declare function initLogTabs(): void;
export declare function loadLogsData(logType?: string, searchParams?: LogSearchParams): Promise<void>;
export declare function loadNotificationsData(filterStatus?: string, page?: number): Promise<void>;
export declare function markNotificationAsRead(notificationId: string): Promise<void>;
export declare function clearReadNotifications(): Promise<void>;
export declare function saveMacNotificationEmail(): Promise<void>;
export {};
//# sourceMappingURL=log.d.ts.map