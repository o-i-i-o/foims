interface SwitchPort {
    id: string;
    port_number: string;
    name?: string;
    status: string;
    speed?: string;
    duplex?: string;
    vlan_id?: number;
    description?: string;
    connected_device?: string;
}
export declare function loadSwitchPortsData(switchId: string, page?: number): Promise<void>;
export declare function loadSwitchPortsBySwitchId(switchId: string): Promise<SwitchPort[]>;
export declare function manageSwitchPorts(switchId: string): void;
export declare function openSwitchPortModal(switchId: string): void;
export declare function editSwitchPort(portId: string): Promise<void>;
export declare function deleteSwitchPort(portId: string | number): Promise<boolean>;
export declare function submitSwitchPortForm(): Promise<boolean>;
export declare function groupPorts(ports: SwitchPort[]): Record<string, SwitchPort[]>;
export declare function showPortGroupsModal(switchId: string): void;
export declare function extractPortNumber(portName: string): number;
export declare function extractPortLastNumber(portName: string): number;
export {};
//# sourceMappingURL=switchPort.d.ts.map