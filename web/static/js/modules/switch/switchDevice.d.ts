import { toggleSnmpConfig, testSnmpConnection, syncPortsFromSnmp } from "./switchSnmp.js";
import { SWITCH_PAGE_SIZE, SWITCH_PORT_PAGE_SIZE } from "./switchState.js";
import { loadSwitchesData, deleteSwitch, submitSwitchForm } from "./switchList.js";
import { loadSwitchPortsData, deleteSwitchPort } from "./switchPort.js";
import { viewArpTable, viewLldpNeighbors } from "./switchMacLldp.js";
export declare function openSwitchModal(sw?: Record<string, unknown> | null): Promise<void>;
export declare function editSwitch(id: string | number): Promise<void>;
export { deleteSwitch, deleteSwitchPort };
export declare function initSwitchEvents(): void;
export { loadSwitchesData, loadSwitchPortsData, submitSwitchForm, toggleSnmpConfig, testSnmpConnection, syncPortsFromSnmp, viewArpTable, viewLldpNeighbors, SWITCH_PAGE_SIZE, SWITCH_PORT_PAGE_SIZE, };
//# sourceMappingURL=switchDevice.d.ts.map