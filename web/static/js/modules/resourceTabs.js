import { loadNetworkTypesData, loadNetworksData, initNetworksSearch } from "./networks.js";
import { loadRoomsData, initRoomSortEvents } from "./room.js";
import { loadWorkstationsData, initWorkstationSortEvents } from "./workstation.js";
import { loadCabinetsData, initCabinetSortEvents } from "./cabinet.js";
import { loadCabinetPositionsData, initCabinetPositionSortEvents } from "./position.js";
import { nextFrame, safeAsync } from "../utils/helpers.js";
const TAB_DATA_LOADERS = {
    rooms: loadRoomsData,
    workstations: loadWorkstationsData,
    networks: async () => {
        loadNetworkTypesData();
        loadNetworksData();
    },
    switches: async () => { },
    cabinets: loadCabinetsData,
    "cabinet-positions": loadCabinetPositionsData,
};
export function initResourceTabs() {
    const resourcesContainer = document.getElementById("resources");
    if (!resourcesContainer) {
        return;
    }
    if (isAlreadyInitialized(resourcesContainer)) {
        return;
    }
    initNetworksSearch();
    initRoomSortEvents();
    initWorkstationSortEvents();
    initCabinetSortEvents();
    initCabinetPositionSortEvents();
    bindTabClickHandlers(resourcesContainer);
    markAsInitialized(resourcesContainer);
    loadDefaultTabData(resourcesContainer);
}
function bindTabClickHandlers(container) {
    const tabBtns = container.querySelectorAll(".tab-btn");
    const tabContents = container.querySelectorAll(".tab-content");
    tabBtns.forEach((btn) => {
        btn.addEventListener("click", function () {
            const tabId = this.getAttribute("data-tab");
            updateActiveTab(tabBtns, tabContents, this, tabId);
            loadTabData(tabId);
        });
    });
}
function updateActiveTab(tabBtns, tabContents, activeBtn, tabId) {
    tabBtns.forEach((btn) => btn.classList.remove("active"));
    activeBtn.classList.add("active");
    tabContents.forEach((content) => content.classList.remove("active"));
    const targetTab = document.getElementById(`${tabId}-tab`);
    targetTab?.classList.add("active");
}
function loadTabData(tabId) {
    nextFrame(() => {
        if (!tabId)
            return;
        const loader = TAB_DATA_LOADERS[tabId];
        if (loader) {
            safeAsync(() => Promise.resolve(loader()), `加载${tabId}数据`);
        }
    });
}
function isAlreadyInitialized(container) {
    return container.dataset.tabsInitialized === "true";
}
function markAsInitialized(container) {
    container.dataset.tabsInitialized = "true";
}
function loadDefaultTabData(container) {
    const defaultTabBtn = container.querySelector(".tab-btn.active");
    if (defaultTabBtn) {
        const tabId = defaultTabBtn.getAttribute("data-tab");
        const loader = tabId ? TAB_DATA_LOADERS[tabId] : null;
        if (loader) {
            safeAsync(() => Promise.resolve(loader()), `加载${tabId}数据`);
        }
    }
}
//# sourceMappingURL=resourceTabs.js.map