export const SWITCH_PAGE_SIZE = 20;
export const SWITCH_PORT_PAGE_SIZE = 50;

interface ListState {
  currentPage: number;
  currentSwitchId: number | string | null;
  currentSwitchName: string | null;
}

interface PositionState {
  cabinetId: number | string | null;
  cabinetName: string | null;
  startU: number | null;
  endU: number | null;
  positionId: number | string | null;
  networkRegionId: number | string | null;
}

interface NetworkRegionState {
  id: number | string | null;
  name: string;
}

const listState: ListState = {
  currentPage: 1,
  currentSwitchId: null,
  currentSwitchName: null,
};

const positionData: PositionState = {
  cabinetId: null,
  cabinetName: null,
  startU: null,
  endU: null,
  positionId: null,
  networkRegionId: null,
};

const networkRegion: NetworkRegionState = {
  id: null,
  name: "",
};

export function createPositionState(): PositionState {
  return positionData;
}

export function createNetworkRegionState(): NetworkRegionState {
  return networkRegion;
}

export function setCurrentSwitchPage(page: number): void {
  listState.currentPage = page;
}

export function getCurrentSwitchPage(): number {
  return listState.currentPage;
}

export function setCurrentSwitchId(id: number | string | null): void {
  listState.currentSwitchId = id;
}

export function getCurrentSwitchId(): number | string | null {
  return listState.currentSwitchId;
}

export function setCurrentSwitchName(name: string | null): void {
  listState.currentSwitchName = name;
}

export function getCurrentSwitchName(): string | null {
  return listState.currentSwitchName;
}

export function getSwitchPositionData(): PositionState {
  return positionData;
}

export function resetSwitchPositionData(): void {
  Object.assign(positionData, {
    cabinetId: null,
    cabinetName: null,
    startU: null,
    endU: null,
    positionId: null,
    networkRegionId: null,
  });
}

export function updateNetworkRegion(id: number | string | null, name = ""): void {
  networkRegion.id = id;
  networkRegion.name = name;
}

export function getNetworkRegion(): NetworkRegionState {
  return networkRegion;
}

export { listState, positionData, networkRegion };
