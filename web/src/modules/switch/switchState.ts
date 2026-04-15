export const SWITCH_PAGE_SIZE = 20;
export const SWITCH_PORT_PAGE_SIZE = 20;

interface SwitchListState {
  currentSwitchId: string | null;
  currentSwitchName: string;
  currentSwitchPage: number;
  networkRegionId: string | null;
  networkRegionName: string;
}

export const listState: SwitchListState = {
  currentSwitchId: null,
  currentSwitchName: "",
  currentSwitchPage: 1,
  networkRegionId: null,
  networkRegionName: "",
};

interface PositionData {
  cabinetId: string | null;
  cabinetName: string | null;
  startU: number | null;
  endU: number | null;
  positionId: string | null;
  networkRegionId: string | null;
}

export const positionData: PositionData = {
  cabinetId: null,
  cabinetName: null,
  startU: null,
  endU: null,
  positionId: null,
  networkRegionId: null,
};

interface NetworkRegionState {
  id: string | null;
  name: string;
}

export const networkRegion: NetworkRegionState = {
  id: null,
  name: "",
};

export function updateNetworkRegion(regionId: string | null, regionName: string): void {
  listState.networkRegionId = regionId;
  listState.networkRegionName = regionName;
}

export function setCurrentSwitchId(id: string | null): void {
  listState.currentSwitchId = id;
}

export function setCurrentSwitchName(name: string): void {
  listState.currentSwitchName = name;
}

export function setCurrentSwitchPage(page: number): void {
  listState.currentSwitchPage = page;
}

export function getCurrentSwitchId(): string | null {
  return listState.currentSwitchId;
}

export function getCurrentSwitchName(): string {
  return listState.currentSwitchName;
}
