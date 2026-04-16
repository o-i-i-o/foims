export declare function loadUsersData(page?: number): Promise<void>;
declare global {
    interface Window {
        openUserModal: (userId?: string | null) => void;
        openTwoFactorModal: (userId: string, username: string, isEnabled: boolean) => Promise<void>;
    }
}
export declare function openUserModal(userId?: string | null): void;
export declare function deleteUser(userId: string | number): Promise<void>;
declare function initUserEvents(): void;
export { initUserEvents };
export declare function submitUserForm(): Promise<void>;
//# sourceMappingURL=userManager.d.ts.map