import {
  createListState,
} from "./switchList.js";

import {
  createPositionState,
} from "./switchPosition.js";

const listState = createListState();
const positionData = createPositionState();

export function setCurrentSwitchPage(page) { listState.currentPage = page; }
export function getCurrentSwitchPage() { return listState.currentPage; }
export function setCurrentSwitchId(id) { listState.currentSwitchId = id; }
export function getCurrentSwitchId() { return listState.currentSwitchId; }
export function setCurrentSwitchName(name) { listState.currentSwitchName = name; }
export function getCurrentSwitchName() { return listState.currentSwitchName; }
export function getSwitchPositionData() { return positionData; }
export function resetSwitchPositionData() {
  Object.assign(positionData, createPositionState());
}

export { listState, positionData };
