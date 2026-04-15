// Switch position selector
export class PositionSelector {
  private onSelect: (regionId: string | null, regionName: string) => void;

  constructor(onSelect: (regionId: string | null, regionName: string) => void) {
    this.onSelect = onSelect;
  }

  async init(sw?: Record<string, unknown> | null): Promise<void> {
    // TODO: Implement position selector initialization
    console.log("PositionSelector init", sw);
    // Use onSelect to avoid unused parameter warning
    this.onSelect(null, "");
  }

  destroy(): void {
    // TODO: Implement cleanup
  }
}
