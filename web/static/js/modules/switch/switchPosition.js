// Switch position selector
export class PositionSelector {
    onSelect;
    constructor(onSelect) {
        this.onSelect = onSelect;
    }
    async init(sw) {
        // TODO: Implement position selector initialization
        console.log("PositionSelector init", sw);
        // Use onSelect to avoid unused parameter warning
        this.onSelect(null, "");
    }
    destroy() {
        // TODO: Implement cleanup
    }
}
//# sourceMappingURL=switchPosition.js.map