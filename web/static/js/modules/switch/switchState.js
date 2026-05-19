export const SWITCH_PAGE_SIZE = 20;
export const SWITCH_PORT_PAGE_SIZE = 50;

const listState = {
  currentPage: 1,
  currentSwitchId: null,
  currentSwitchName: null
};

const positionData = {
  roomId: null,
  cabinetId: null,
  cabinetName: null,
  startU: null,
  endU: null,
  positionId: null,
  networkRegionId: null
};

const networkRegion = {
  id: null,
  name: ''
};

export function setCurrentSwitchPage(page) {
  listState.currentPage = page;
}

export function getCurrentSwitchPage() {
  return listState.currentPage;
}

export function setCurrentSwitchId(id) {
  listState.currentSwitchId = id;
}

export function getCurrentSwitchId() {
  return listState.currentSwitchId;
}

export function setCurrentSwitchName(name) {
  listState.currentSwitchName = name;
}

export function getCurrentSwitchName() {
  return listState.currentSwitchName;
}

export function updateNetworkRegion(id, name = '') {
  networkRegion.id = id;
  networkRegion.name = name;
}

export { listState, positionData, networkRegion };
