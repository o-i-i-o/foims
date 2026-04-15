export * from "./constants.js";

export { apiGet, apiPost, apiPut, apiDelete } from "./apiClient.js";

export { showToast, showSuccess, showError, showWarning, showInfo } from "./toast.js";

export { openModal, closeModal } from "./modal.js";

export { showConfirm, confirmDelete } from "./confirm.js";

export { t, getCurrentLanguage } from "./i18n.js";

export { escapeHtml, elementCache } from "./helpers.js";

export { formatDateTime } from "./formatter.js";

export {
  renderTable,
  appendPaginationToTable,
  handleError,
  createSortState,
  updateSortIcons,
  initSortEvents,
  debounce,
  throttle,
  DEFAULT_PAGE_SIZE,
  showLoading,
  hideLoading,
  setLoading,
  highlightElement,
  copyToClipboard,
  getElementValue,
  handleFormSubmit,
  handleDelete,
} from "./ui.js";

export {
  loadRoomsForSelect,
  loadDataCenterRoomsForSelect,
  loadRoomNetworksForCabinet,
  loadCabinets,
  loadNetworkTypeOptions,
  loadNetworkRegionsForSelect,
  formatMacAddress,
} from "./resources.js";

export { createCrudManager } from "./crudFactory.js";

export {
  createNetworkRegionManager,
  createNetworkManager,
  createRoomManager,
  createCabinetManager,
  createCabinetPositionManager,
  createWorkstationManager,
  createSwitchManager,
  createUserManager,
  managers,
  roomManager,
  cabinetManager,
  cabinetPositionManager,
  workstationManager,
  switchManager,
  userManager,
  networkRegionManager,
  networkManager,
} from "./managers.js";

export {
  templateManager,
  createRowTemplate,
  createButtonTemplate,
  commonTemplates,
  renderEmptyRow,
  renderStatusBadge,
  renderActionButtons,
} from "./templateManager.js";

export type { TemplateData, TemplateCache } from "./templateManager.js";

export {
  eventDelegator,
  setupTableEvents,
  setupFormEvents,
  setupPaginationEvents,
  setupSearchEvents,
  setupSortEvents,
} from "./eventDelegator.js";

export type { EventHandler, DelegatedEvent } from "./eventDelegator.js";

export {
  errorHandler,
  handleApiError,
  handleNetworkError,
  handleValidationError,
  handleAuthError,
  handlePermissionError,
  handleNotFoundError,
  handleTimeoutError,
  wrapAsync,
  withErrorHandling,
  ErrorCodes,
} from "./errorHandler.js";

export type { AppError, ErrorSeverity, ErrorHandlerOptions } from "./errorHandler.js";

export { SessionManager, getUser, setUser, clearSession, hasSession, isRememberMe } from "./sessionManager.js";

export { loadModule } from "./moduleLoader.js";

export { loadModal } from "./modalLoader.js";

export { renderPagination } from "./pagination.js";

export {
  getManager,
  handleCabinetPositionCabinetChange,
  handleWorkstationRoomChange,
} from "./ipconfig.js";
