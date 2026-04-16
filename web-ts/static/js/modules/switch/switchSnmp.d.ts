export declare function toggleSnmpConfig(): void;
export declare function testSnmpConnection(_switchId?: string): Promise<boolean>;
export declare function getSwitchInfoFromSnmp(): Promise<Record<string, unknown> | null>;
export declare function syncPortsFromSnmp(switchId: string): Promise<boolean>;
//# sourceMappingURL=switchSnmp.d.ts.map