export const SWITCH_PAGE_SIZE = 20;
export const SWITCH_PORT_PAGE_SIZE = 20;
export const listState = {
    currentSwitchId: null,
    currentSwitchName: "",
    currentSwitchPage: 1,
    networkRegionId: null,
    networkRegionName: "",
};
export const positionData = {
    cabinetId: null,
    cabinetName: null,
    startU: null,
    endU: null,
    positionId: null,
    networkRegionId: null,
};
export const networkRegion = {
    id: null,
    name: "",
};
export function updateNetworkRegion(regionId, regionName) {
    listState.networkRegionId = regionId;
    listState.networkRegionName = regionName;
}
export function setCurrentSwitchId(id) {
    listState.currentSwitchId = id;
}
export function setCurrentSwitchName(name) {
    listState.currentSwitchName = name;
}
export function setCurrentSwitchPage(page) {
    listState.currentSwitchPage = page;
}
export function getCurrentSwitchId() {
    return listState.currentSwitchId;
}
export function getCurrentSwitchName() {
    return listState.currentSwitchName;
}
//# sourceMappingURL=switchState.js.map