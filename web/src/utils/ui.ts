import type { SortOrder, SortState, TableOptions, ColumnDef } from "../types/common.js";
import type { ApiResponse } from "../types/api.js";

export { showToast, showSuccess, showError, showWarning, showInfo } from "./toast.js";
export { showConfirm, confirmDelete } from "./confirm.js";
export { renderPagination, renderPageInfo, createPaginationState } from "./pagination.js";
export { formatDateTime, formatDate, formatRelativeTime } from "./formatter.js";

export const DEFAULT_PAGE_SIZE = 20;

import { renderPagination as renderPaginationFn } from "./pagination.js";
import { showToast as showToastFn } from "./toast.js";
import { closeModal as closeModalFn } from "./modal.js";
import { apiPost, apiPut, apiDelete } from "./apiClient.js";
import { showConfirm } from "./confirm.js";
import { escapeHtml } from "./helpers.js";

export { escapeHtml };

export function createSortState(defaultBy = "name", defaultOrder: SortOrder = "asc"): SortState {
  let by = defaultBy;
  let order: SortOrder = defaultOrder;

  return {
    get by() { return by; },
    get order() { return order; },
    toggle(key: string): SortState {
      if (by === key) {
        order = order === "asc" ? "desc" : "asc";
      } else {
        by = key;
        order = "asc";
      }
      return this;
    },
    setSort(newBy: string, newOrder: SortOrder | null = null): SortState {
      by = newBy;
      if (newOrder) {
        order = newOrder;
      }
      return this;
    },
    get sortBy(): string { return by; },
    get sortOrder(): SortOrder { return order; },
  };
}

export function updateSortIcons(tableId: string | HTMLElement, sortState: SortState): void {
  const table = typeof tableId === "string" ? document.getElementById(tableId) : tableId;
  if (!table) return;

  table.querySelectorAll<HTMLElement>("th.sortable").forEach(th => {
    const sortKey = th.dataset.sort;

    if (sortKey === sortState.sortBy) {
      th.classList.add("sorted", sortState.sortOrder);
      th.classList.remove(sortState.sortOrder === "asc" ? "desc" : "asc");
    } else {
      th.classList.remove("sorted", "asc", "desc");
    }
  });
}

export function initSortEvents(tableId: string | HTMLElement, sortState: SortState, loadDataFn: (page: number, sortBy: string, sortOrder: string) => void): void {
  const table = typeof tableId === "string" ? document.getElementById(tableId) : tableId;
  if (!table) return;

  table.querySelectorAll<HTMLElement>("th.sortable").forEach(th => {
    th.addEventListener("click", () => {
      const sortKey = th.dataset.sort;
      if (sortKey) {
        sortState.toggle(sortKey);
        loadDataFn(1, sortState.sortBy, sortState.sortOrder);
      }
    });
  });
}

export function createTableState(options: { pageSize?: number; defaultSortBy?: string; defaultSortOrder?: SortOrder } = {}) {
  const {
    pageSize = DEFAULT_PAGE_SIZE,
    defaultSortBy = "name",
    defaultSortOrder = "asc" as SortOrder,
  } = options;

  let isLoading = false;
  let currentPage = 1;
  const sortState = createSortState(defaultSortBy, defaultSortOrder);

  return {
    get isLoading() { return isLoading; },
    get currentPage() { return currentPage; },
    get sortBy() { return sortState.sortBy; },
    get sortOrder() { return sortState.sortOrder; },
    get pageSize() { return pageSize; },
    get sortState() { return sortState; },

    setLoading(value: boolean) { isLoading = value; },
    setPage(page: number) { currentPage = page; return this; },
    setSort(by: string, order: SortOrder) { sortState.setSort(by, order); return this; },

    toggleSort(key: string) {
      sortState.toggle(key);
      currentPage = 1;
      return this;
    },

    getQueryParams(): Record<string, string | number> {
      return {
        page: currentPage,
        page_size: pageSize,
        sort_by: sortState.sortBy,
        sort_order: sortState.sortOrder,
      };
    },
  };
}

export function debounce<T extends (...args: unknown[]) => void>(func: T, wait: number): (...args: Parameters<T>) => void {
  let timeout: ReturnType<typeof setTimeout>;
  return function executedFunction(...args: Parameters<T>) {
    const later = () => {
      clearTimeout(timeout);
      func(...args);
    };
    clearTimeout(timeout);
    timeout = setTimeout(later, wait);
  };
}

export function throttle<T extends (...args: unknown[]) => void>(func: T, limit: number): (...args: Parameters<T>) => void {
  let inThrottle: boolean;
  return function executedFunction(...args: Parameters<T>) {
    if (!inThrottle) {
      func(...args);
      inThrottle = true;
      setTimeout(() => inThrottle = false, limit);
    }
  };
}

export function sanitizeHtml(html: string | null | undefined): string {
  if (!html) return "";
  const div = document.createElement("div");
  div.textContent = html;
  return div.innerHTML;
}

interface ElementAttributes {
  className?: string;
  [key: string]: unknown;
}

export function createElement(tag: string, attributes: ElementAttributes = {}, children: (string | Node)[] = []): HTMLElement {
  const element = document.createElement(tag);

  Object.entries(attributes).forEach(([key, value]) => {
    if (key === "className") {
      element.className = value as string;
    } else if (key.startsWith("data-")) {
      element.dataset[key.slice(5)] = value as string;
    } else if (key.startsWith("on") && typeof value === "function") {
      const eventName = key.slice(2).toLowerCase();
      element.addEventListener(eventName, value as EventListener);
    } else {
      (element as unknown as Record<string, unknown>)[key] = value;
    }
  });

  if (children.length > 0) {
    children.forEach(child => {
      if (typeof child === "string") {
        element.insertAdjacentHTML("beforeend", child);
      } else {
        element.appendChild(child);
      }
    });
  }

  return element;
}

export function renderTable<T extends Record<string, unknown>>(
  container: string | HTMLElement,
  dataOrOptions: T[] | TableOptions<T>,
  renderFn?: (row: T, index: number) => string,
  emptyMessage?: string,
  colSpan?: number,
): void {
  const el = typeof container === "string" ? document.querySelector<HTMLElement>(container) : container;

  if (!el) return;

  let data: T[];
  let columns: ColumnDef<T>[] = [];
  let empty: string;
  let colspan: number;

  if (Array.isArray(dataOrOptions)) {
    data = dataOrOptions;
    empty = emptyMessage || "暂无数据";
    colspan = colSpan || 1;

    const tbody = el.querySelector("tbody") || el;

    if (!data.length) {
      tbody.innerHTML = `<tr><td colspan="${colspan}" class="text-center text-muted">${empty}</td></tr>`;
      return;
    }

    tbody.innerHTML = "";
    data.forEach((row, index) => {
      const tr = document.createElement("tr");
      if (renderFn) {
        tr.innerHTML = renderFn(row, index);
      }
      tbody.appendChild(tr);
    });
  } else if (typeof dataOrOptions === "object" && dataOrOptions !== null) {
    const options = dataOrOptions as TableOptions<T>;
    data = options.data || [];
    columns = options.columns || [];
    empty = options.emptyMessage || "暂无数据";
    const rowIdField = options.rowIdField;
    const onRowClick = options.onRowClick;
    const onRowDoubleClick = options.onRowDoubleClick;

    const tbody = el.querySelector("tbody") || el;
    const thead = el.querySelector("thead");
    const headerColumnCount = thead ? thead.querySelectorAll("th").length : columns.length;

    if (!data.length) {
      tbody.innerHTML = `<tr><td colspan="${headerColumnCount || columns.length || 1}" class="text-center text-muted">${empty}</td></tr>`;
      return;
    }

    const fragment = document.createDocumentFragment();

    data.forEach((row, index) => {
      const tr = document.createElement("tr");

      if (rowIdField && row[rowIdField]) {
        tr.dataset.id = String(row[rowIdField]);
      }

      columns.forEach(column => {
        const td = document.createElement("td");

        if (column.render) {
          const content = column.render(row[column.field], row, index);
          if (typeof content === "string") {
            td.innerHTML = content;
          } else if (content instanceof Node) {
            td.appendChild(content);
          } else if (content !== undefined && content !== null) {
            td.textContent = String(content);
          }
        } else {
          td.textContent = row[column.field] != null ? String(row[column.field]) : "";
        }

        if (column.className) {
          td.className = column.className;
        }

        tr.appendChild(td);
      });

      if (onRowClick) {
        tr.addEventListener("click", () => onRowClick(row, index));
      }

      if (onRowDoubleClick) {
        tr.addEventListener("dblclick", () => onRowDoubleClick(row, index));
      }

      fragment.appendChild(tr);
    });

    tbody.innerHTML = "";
    tbody.appendChild(fragment);
  }
}

export function showLoading(container: HTMLElement, message = "加载中..."): void {
  if (!container) return;

  container.innerHTML = `
        <div class="loading-overlay">
            <div class="loading-spinner"></div>
            <span class="loading-text">${message}</span>
        </div>
    `;
}

export function hideLoading(container: HTMLElement | null): void {
  const loading = container?.querySelector(".loading-overlay");
  if (loading) {
    loading.remove();
  }
}

export function setLoading(element: HTMLElement | null, isLoading: boolean): void {
  if (!element) return;

  if (isLoading) {
    element.classList.add("loading");
    (element as HTMLButtonElement).disabled = true;
  } else {
    element.classList.remove("loading");
    (element as HTMLButtonElement).disabled = false;
  }
}

export function highlightElement(element: HTMLElement | null, duration = 2000): void {
  if (!element) return;

  element.classList.add("highlight");
  setTimeout(() => {
    element.classList.remove("highlight");
  }, duration);
}

export function copyToClipboard(text: string): Promise<boolean> {
  return navigator.clipboard.writeText(text).then(() => {
    return true;
  }).catch(() => {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.select();

    try {
      document.execCommand("copy");
      document.body.removeChild(textarea);
      return true;
    } catch {
      document.body.removeChild(textarea);
      return false;
    }
  });
}

export function getElementValue(id: string): string | boolean {
  const element = document.getElementById(id) as HTMLInputElement | null;
  if (!element) return "";

  if (element.type === "checkbox") {
    return element.checked;
  }

  return element.value?.trim() ?? "";
}

export function handleError(error: unknown, defaultMessage = "操作失败"): void {
  console.error("Error:", error);

  if (error instanceof Error && error.message) {
    showToastFn(error.message, "error");
  } else if (typeof error === "string") {
    showToastFn(error, "error");
  } else {
    showToastFn(defaultMessage, "error");
  }
}

interface HandleFormSubmitConfig {
  formData: Record<string, unknown>;
  id?: number | string | null;
  baseUrl: string;
  successMessage?: string;
  errorMessage?: string;
  modalId?: string;
  reloadFunction?: () => Promise<void>;
}

export async function handleFormSubmit(config: HandleFormSubmitConfig): Promise<boolean> {
  const {
    formData,
    id,
    baseUrl,
    successMessage = "保存成功",
    errorMessage = "保存失败",
    modalId,
    reloadFunction,
  } = config;

  try {
    let result: ApiResponse;
    if (id) {
      result = await apiPut(`${baseUrl}/${id}`, formData);
    } else {
      result = await apiPost(baseUrl, formData);
    }

    if (result.success) {
      showToastFn(successMessage, "success");

      if (modalId) {
        closeModalFn(modalId);
      }

      if (reloadFunction) {
        await reloadFunction();
      }

      return true;
    } else {
      showToastFn(result.message || errorMessage, "error");
      return false;
    }
  } catch (error) {
    handleError(error, errorMessage);
    return false;
  }
}

interface HandleDeleteOptions {
  confirmMessage?: string;
  errorMessage?: string;
  successMessage?: string;
}

export async function handleDelete(
  id: number | string,
  apiOrCallback: string | ((id: number | string) => Promise<ApiResponse>),
  successMessageOrOptions?: string | HandleDeleteOptions,
  callbackOrOptions?: (() => Promise<void>) | HandleDeleteOptions,
): Promise<{ success: boolean; cancelled?: boolean; message?: string }> {
  let apiPath: string | null = null;
  let successMessage: string;
  let refreshCallback: (() => Promise<void>) | null = null;
  let options: HandleDeleteOptions = {};

  if (typeof apiOrCallback === "string") {
    apiPath = apiOrCallback;
    if (typeof successMessageOrOptions === "string") {
      successMessage = successMessageOrOptions;
      options = typeof callbackOrOptions === "object" ? callbackOrOptions : {};
    } else {
      successMessage = "删除成功";
      options = successMessageOrOptions || {};
    }
    refreshCallback = typeof callbackOrOptions === "function" ? callbackOrOptions : null;
  } else {
    options = (successMessageOrOptions as HandleDeleteOptions) || {};
    successMessage = options.successMessage || "删除成功";
  }

  const confirmMessage = options.confirmMessage || "确定要删除吗？";
  const errorMsg = options.errorMessage || "删除失败";

  const confirmed = await showConfirm(confirmMessage);
  if (!confirmed) return { success: false, cancelled: true };

  try {
    let result: ApiResponse;
    if (apiPath) {
      result = await apiDelete(`${apiPath}/${id}`);
    } else if (typeof apiOrCallback === "function") {
      result = await apiOrCallback(id);
    } else {
      throw new Error("No delete callback or API path provided");
    }

    if (result.success) {
      showToastFn(successMessage, "success");
      if (refreshCallback) {
        await refreshCallback();
      }
      return { success: true };
    } else {
      showToastFn(result.message || errorMsg, "error");
      return { success: false, message: result.message };
    }
  } catch (error) {
    handleError(error, errorMsg);
    return { success: false, message: (error as Error).message };
  }
}

interface PaginationData {
  total?: number;
  page?: number;
  page_size?: number;
}

export function appendPaginationToTable(container: string | HTMLElement, data: PaginationData, onPageChange: (page: number) => void): void {
  const el = typeof container === "string" ? document.querySelector<HTMLElement>(container) : container;

  if (!el) return;

  const tableContainer = el.closest(".table-container");
  let paginationContainer: HTMLElement;

  if (tableContainer) {
    const existingPagination = tableContainer.querySelector(".pagination-container");
    if (existingPagination) {
      existingPagination.remove();
    }
    paginationContainer = document.createElement("div");
    paginationContainer.className = "pagination-container";
    tableContainer.appendChild(paginationContainer);
  } else {
    const existingPagination = el.querySelector(".pagination-wrapper");
    if (existingPagination) {
      existingPagination.remove();
    }
    paginationContainer = el;
  }

  const total = data.total || 0;
  const currentPage = data.page || 1;
  const pageSize = data.page_size || 20;
  const totalPages = Math.ceil(total / pageSize);

  if (totalPages <= 1) {
    if (paginationContainer && paginationContainer.classList.contains("pagination-container")) {
      paginationContainer.innerHTML = "";
    }
    return;
  }

  const paginationWrapper = document.createElement("div");
  paginationWrapper.className = "pagination-wrapper";

  renderPaginationFn(paginationWrapper, currentPage, totalPages, onPageChange, total);

  paginationContainer.innerHTML = "";
  paginationContainer.appendChild(paginationWrapper);
}

export function removeToast(): void {
  const toasts = document.querySelectorAll(".toast");
  toasts.forEach(toast => {
    toast.classList.remove("toast-visible");
    setTimeout(() => {
      if (toast.parentNode) {
        toast.parentNode.removeChild(toast);
      }
    }, 300);
  });
}
