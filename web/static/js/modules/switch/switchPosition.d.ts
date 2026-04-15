export declare class PositionSelector {
    private onSelect;
    constructor(onSelect: (regionId: string | null, regionName: string) => void);
    init(sw?: Record<string, unknown> | null): Promise<void>;
    destroy(): void;
}
//# sourceMappingURL=switchPosition.d.ts.map