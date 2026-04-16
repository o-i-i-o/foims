import { toggleSnmpConfig, testSnmpConnection, getSwitchInfoFromSnmp, syncPortsFromSnmp } from "./switchSnmp.js";
import { setCurrentSwitchId, setCurrentSwitchPage, setCurrentSwitchName, getCurrentSwitchId, getCurrentSwitchName, SWITCH_PAGE_SIZE, SWITCH_PORT_PAGE_SIZE } from "./switchState.js";
import { loadSwitchesData, deleteSwitch } from "./switchList.js";
import { loadSwitchPortsData, loadSwitchPortsBySwitchId, manageSwitchPorts, openSwitchPortModal, editSwitchPort, deleteSwitchPort, submitSwitchPortForm, groupPorts, showPortGroupsModal, extractPortNumber, extractPortLastNumber } from "./switchPort.js";
import { viewArpTable, viewLldpNeighbors, loadSwitchesForLldp, renderMacTable, bindCollapseEvents, groupByNetwork, filterEntries } from "./switchMacLldp.js";
export declare function openSwitchModal(sw?: Record<string, unknown> | null): Promise<void>;
export declare function editSwitch(id: string | number): Promise<void>;
declare function saveSwitch(): Promise<void>;
declare function initSwitchTabs(): void;
declare function initSwitchSearch(): void;
declare function initSwitches(): void;
export { initSwitches, initSwitchTabs, initSwitchSearch, loadSwitchesData, deleteSwitch, saveSwitch as submitSwitchForm, toggleSnmpConfig, testSnmpConnection, getSwitchInfoFromSnmp, syncPortsFromSnmp, loadSwitchPortsData, loadSwitchPortsBySwitchId, manageSwitchPorts, openSwitchPortModal, editSwitchPort, deleteSwitchPort, submitSwitchPortForm, groupPorts, showPortGroupsModal, extractPortNumber, extractPortLastNumber, viewArpTable, viewLldpNeighbors, loadSwitchesForLldp, renderMacTable, bindCollapseEvents, groupByNetwork, filterEntries, setCurrentSwitchId, setCurrentSwitchName, setCurrentSwitchPage, getCurrentSwitchId, getCurrentSwitchName, SWITCH_PAGE_SIZE, SWITCH_PORT_PAGE_SIZE, };
//# sourceMappingURL=switchDevice.d.ts.map