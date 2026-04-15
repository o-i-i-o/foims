import { loadSwitchesData, initSwitchSearch } from "./switch/switchDevice.js";
import { loadNetworkTypesData, loadNetworksData, initNetworksSearch } from "./networks.js";
import { loadRoomsData, initRoomSortEvents } from "./room.js";
import { loadWorkstationsData, initWorkstationSortEvents } from "./workstation.js";
import { loadCabinetsData, initCabinetSortEvents } from "./cabinet.js";
import { loadCabinetPositionsData, initPositionSortEvents } from "./position.js";
import { nextFrame, safeAsync } from "../utils/helpers.js";

const TAB_DATA_LOADERS: Record<string, () => Promise<void>> = {
  rooms: loadRoomsData,
  workstations: loadWorkstationsData,
  networks: async () => {
    loadNetworkTypesData();
    loadNetworksData();
  },
  switches: loadSwitchesData,
  cabinets: loadCabinetsData,
  "cabinet-positions": loadCabinetPositionsData,
};

export function initResourceTabs(): void {
  const resourcesContainer = document.getElementById("resources");

  if (!resourcesContainer) {
    return;
  }

  if (isAlreadyInitialized(resourcesContainer)) {
    return;
  }

  initNetworksSearch();
  initSwitchSearch();
  initRoomSortEvents();
  initWorkstationSortEvents();
  initCabinetSortEvents();
  initPositionSortEvents();
  bindTabClickHandlers(resourcesContainer);
  markAsInitialized(resourcesContainer);
  loadDefaultTabData(resourcesContainer);
}

function bindTabClickHandlers(container: HTMLElement): void {
  const tabBtns = container.querySelectorAll(".tab-btn");
  const tabContents = container.querySelectorAll(".tab-content");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", function () {
      const tabId = this.getAttribute("data-tab");

      updateActiveTab(tabBtns, tabContents, this as HTMLElement, tabId);
      loadTabData(tabId);
    });
  });
}

function updateActiveTab(tabBtns: NodeListOf<Element>, tabContents: NodeListOf<Element>, activeBtn: HTMLElement, tabId: string | null): void {
  tabBtns.forEach((btn) => btn.classList.remove("active"));
  activeBtn.classList.add("active");

  tabContents.forEach((content) => content.classList.remove("active"));

  const targetTab = document.getElementById(`${tabId}-tab`);
  targetTab?.classList.add("active");
}

function loadTabData(tabId: string | null): void {
  nextFrame(() => {
    if (!tabId) return;
    const loader = TAB_DATA_LOADERS[tabId];

    if (loader) {
      safeAsync(loader, `加载标签 ${tabId} 数据`);
    }
  });
}

function isAlreadyInitialized(container: HTMLElement): boolean {
  return container.dataset.tabsInitialized === "true";
}

function markAsInitialized(container: HTMLElement): void {
  container.dataset.tabsInitialized = "true";
}

function loadDefaultTabData(container: HTMLElement): void {
  const activeTabBtn = container.querySelector(".tab-btn.active");
  const defaultTabBtn = activeTabBtn || container.querySelector(".tab-btn");

  if (defaultTabBtn) {
    const tabId = defaultTabBtn.getAttribute("data-tab");
    loadTabData(tabId);
  }
}
