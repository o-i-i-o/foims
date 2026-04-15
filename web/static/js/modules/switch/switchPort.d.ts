interface SwitchPort {
    id: string;
    switch_id: string;
    switch_name?: string;
    switch_ip?: string;
    port_number: string;
    port_name?: string;
    port_type: string;
    vlan_id?: number | null;
    status: string;
    speed?: string | null;
    description?: string | null;
    connected_device?: string;
    [key: string]: unknown;
}
export declare function loadSwitchPortsData(page?: number, searchTerm?: string): Promise<void>;
export declare function loadSwitchPortsBySwitchId(switchId: string): Promise<SwitchPort[]>;
export declare function manageSwitchPorts(switchId: string, switchName?: string): Promise<void>;
export declare function groupPorts(ports: SwitchPort[]): Record<string, SwitchPort[]>;
export declare function showPortGroupsModal(switchName: string, portGroups: Record<string, SwitchPort[]>, switchId: string): void;
export declare function openSwitchPortModal(portData?: SwitchPort | null, switchId?: string | null): void;
export declare function submitSwitchPortForm(): Promise<boolean>;
export declare function editSwitchPort(id: string): Promise<void>;
export declare function deleteSwitchPort(id: string | number): Promise<boolean>;
export declare function extractPortNumber(portNumber: string): number;
export declare function extractPortLastNumber(portNumber: string): string;
export {};
//# sourceMappingURL=switchPort.d.ts.map