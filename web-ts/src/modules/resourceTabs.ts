import { loadNetworkTypesData, loadNetworksData, initNetworksSearch } from "./networks.js";
import { loadRoomsData, initRoomSortEvents } from "./room.js";
import { loadWorkstationsData, initWorkstationSortEvents } from "./workstation.js";
import { loadCabinetsData, initCabinetSortEvents } from "./cabinet.js";
import { loadCabinetPositionsData, initCabinetPositionSortEvents } from "./position.js";
import { loadSwitchesData } from "./switch/switchList.js";
import { nextFrame, safeAsync } from "../utils/helpers.js";

const TAB_DATA_LOADERS: Record<string, () => Promise<void> | void> = {
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
  initRoomSortEvents();
  initWorkstationSortEvents();
  initCabinetSortEvents();
  initCabinetPositionSortEvents();
  bindTabClickHandlers(resourcesContainer);
  markAsInitialized(resourcesContainer);
  loadDefaultTabData(resourcesContainer);
}

function bindTabClickHandlers(container: HTMLElement): void {
  const tabBtns = container.querySelectorAll(".tab-btn");
  const tabContents = container.querySelectorAll(".tab-content");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", function (this: HTMLElement) {
      const tabId = this.getAttribute("data-tab");

      updateActiveTab(tabBtns, tabContents, this, tabId);
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
      safeAsync(() => Promise.resolve(loader()), `加载${tabId}数据`);
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
  const defaultTabBtn = container.querySelector(".tab-btn.active") as HTMLElement | null;
  if (defaultTabBtn) {
    const tabId = defaultTabBtn.getAttribute("data-tab");
    const loader = tabId ? TAB_DATA_LOADERS[tabId] : null;
    if (loader) {
      safeAsync(() => Promise.resolve(loader()), `加载${tabId}数据`);
    }
  }
}
