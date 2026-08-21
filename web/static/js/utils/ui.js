import { showToast } from "./toast.js";
import { showConfirm, confirmDelete } from "./confirm.js";
import { renderPagination } from "./pagination.js";
import { formatDateTime } from "./formatter.js";
import { closeModal, openModal } from "./modalLoader.js";
import { apiPost, apiPut, apiDelete } from "./apiClient.js";
import { escapeHtml } from "./helpers.js";
import { t } from "./i18n.js";

export { showToast } from "./toast.js";
export { showConfirm, confirmDelete } from "./confirm.js";
export { renderPagination } from "./pagination.js";
export { formatDateTime } from "./formatter.js";
export { escapeHtml } from "./helpers.js";

export const DEFAULT_PAGE_SIZE = 20;

export function createSortState(defaultBy = "name", defaultOrder = "asc") {
  return {
    by: defaultBy,
    order: defaultOrder,
    toggle(key) {
      if (this.by === key) {
        this.order = this.order === "asc" ? "desc" : "asc";
      } else {
        this.by = key;
        this.order = "asc";
      }
      return this;
    },
    setSort(by, order = null) {
      this.by = by;
      if (order) {
        this.order = order;
      }
      return this;
    },
    get sortBy() {
      return this.by;
    },
    get sortOrder() {
      return this.order;
    }
  };
}

export function updateSortIcons(tableId, sortState) {
  const table = typeof tableId === "string" ? document.getElementById(tableId) : tableId;
  if (!table) return;

  table.querySelectorAll("th.sortable").forEach((th) => {
    const sortKey = th.dataset.sort;

    if (sortKey === sortState.by) {
      th.classList.add("sorted", sortState.order);
      th.classList.remove(sortState.order === "asc" ? "desc" : "asc");
    } else {
      th.classList.remove("sorted", "asc", "desc");
    }
  });
}

export function initSortEvents(tableId, sortState, loadDataFn) {
  const table = typeof tableId === "string" ? document.getElementById(tableId) : tableId;
  if (!table) return;

  table.querySelectorAll("th.sortable").forEach((th) => {
    th.addEventListener("click", (e) => {
      // 表头内嵌过滤输入框、弹层等控件时不触发排序
      if (e.target.closest("input, select, textarea, button, a, .th-search-popover")) return;
      const sortKey = th.dataset.sort;
      sortState.toggle(sortKey);
      loadDataFn(1, sortState.by, sortState.order);
    });
  });
}

export function debounce(func, wait) {
  let timeout;
  return function executedFunction(...args) {
    const later = () => {
      clearTimeout(timeout);
      func(...args);
    };
    clearTimeout(timeout);
    timeout = setTimeout(later, wait);
  };
}

export function renderTable(container, dataOrOptions, renderFn, emptyMessage, colSpan) {
  const el = typeof container === "string" ? document.querySelector(container) : container;

  if (!el) return;

  let data, columns, empty, colspan;

  if (Array.isArray(dataOrOptions)) {
    data = dataOrOptions;
    empty = emptyMessage || t("common.no_data");
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
    const options = dataOrOptions;
    data = options.data || [];
    columns = options.columns || [];
    empty = options.emptyMessage || t("common.no_data");
    const rowIdField = options.rowIdField || "id";
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
        tr.dataset.id = row[rowIdField];
      }

      columns.forEach((column) => {
        const td = document.createElement("td");

        if (column.render) {
          const content = column.render(row[column.field], row, index);
          if (typeof content === "string") {
            td.innerHTML = content;
          } else if (content instanceof Node) {
            td.appendChild(content);
          } else if (content !== undefined && content !== null) {
            td.textContent = content;
          }
        } else {
          td.textContent = row[column.field] ?? "";
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

export function getElementValue(id) {
  const element = document.getElementById(id);
  if (!element) return "";

  if (element.type === "checkbox") {
    return element.checked;
  }

  return element.value?.trim() ?? "";
}

export function handleError(error, defaultMessage = t("common.operation_failed")) {
  console.error("Error:", error);

  if (error.message) {
    showToast(error.message, "error");
  } else if (typeof error === "string") {
    showToast(error, "error");
  } else {
    showToast(defaultMessage, "error");
  }
}

export async function handleFormSubmit(config) {
  const {
    formData,
    id,
    baseUrl,
    successMessage = t("common.save_success"),
    errorMessage = t("common.save_failed"),
    modalId,
    reloadFunction
  } = config;

  try {
    let result;
    if (id) {
      result = await apiPut(`${baseUrl}/${id}`, formData);
    } else {
      result = await apiPost(baseUrl, formData);
    }

    if (result.success) {
      showToast(successMessage, "success");

      if (modalId) {
        closeModal(modalId);
      }

      if (reloadFunction) {
        await reloadFunction();
      }

      return true;
    } else {
      showToast(result.message || errorMessage, "error");
      return false;
    }
  } catch (error) {
    handleError(error, errorMessage);
    return false;
  }
}

export async function handleDelete(
  id,
  apiOrCallback,
  successMessageOrOptions,
  callbackOrOptions = {}
) {
  let apiPath, successMessage, refreshCallback, options;

  if (typeof apiOrCallback === "string") {
    apiPath = apiOrCallback;
    if (typeof successMessageOrOptions === "string") {
      successMessage = successMessageOrOptions;
      options = typeof callbackOrOptions === "object" ? callbackOrOptions : {};
    } else {
      successMessage = t("common.delete_success");
      options = successMessageOrOptions || {};
    }
    refreshCallback = typeof callbackOrOptions === "function" ? callbackOrOptions : null;
  } else {
    options = successMessageOrOptions || {};
    successMessage = options.successMessage || t("common.delete_success");
  }

  const confirmMessage = options.confirmMessage || t("common.delete_confirm");
  const errorMessage = options.errorMessage || t("common.delete_failed");

  const confirmed = await showConfirm(confirmMessage);
  if (!confirmed) return { success: false, cancelled: true };

  try {
    let result;
    if (apiPath) {
      result = await apiDelete(`${apiPath}/${id}`);
    } else if (typeof apiOrCallback === "function") {
      result = await apiOrCallback(id);
    } else {
      throw new Error("No delete callback or API path provided");
    }

    if (result.success) {
      showToast(successMessage, "success");
      if (refreshCallback) {
        await refreshCallback();
      }
      return { success: true };
    } else {
      showToast(result.message || errorMessage, "error");
      return { success: false, message: result.message };
    }
  } catch (error) {
    handleError(error, errorMessage);
    return { success: false, message: error.message };
  }
}

export function appendPaginationToTable(container, data, onPageChange, options = {}) {
  const el = typeof container === "string" ? document.querySelector(container) : container;

  if (!el) return;

  const tableContainer = el.closest(".table-container");
  let paginationContainer;

  if (tableContainer) {
    let existingPagination = tableContainer.querySelector(".pagination-container");
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
  const pageSize = data.page_size || options.pageSize || 20;
  // Trust the backend's total_pages when provided; only fall back to a local
  // computation when it is missing. Previously this always recomputed
  // Math.ceil(total / 20), which broke callers that use a different page size
  // (e.g. the IP table uses 100) and rendered many phantom empty pages.
  const totalPages =
    data.total_pages != null && data.total_pages > 0
      ? data.total_pages
      : Math.ceil(total / pageSize);

  // Even with a single page we may still want the page-size selector, so only
  // short-circuit when there is nothing interactive to show.
  const hasSizeSelector = typeof options.onPageSizeChange === "function";
  if (totalPages <= 1 && !hasSizeSelector) {
    if (paginationContainer && paginationContainer.classList.contains("pagination-container")) {
      paginationContainer.innerHTML = "";
    }
    return;
  }

  const paginationWrapper = document.createElement("div");
  paginationWrapper.className = "pagination-wrapper";

  renderPagination(paginationWrapper, currentPage, totalPages, onPageChange, total, {
    pageSize,
    pageSizeOptions: options.pageSizeOptions,
    onPageSizeChange: options.onPageSizeChange
  });

  paginationContainer.innerHTML = "";
  paginationContainer.appendChild(paginationWrapper);
}

/**
 * 通用简单列表模态框（模板 modals/common/simple-list-modal.html）。
 * 自动追加序号列，适合工位/信息点/机位这类无复杂交互的只读列表。
 * @param {Object} options
 * @param {string} options.title 标题（通常为 "名称 - 列表类型"）
 * @param {Array<{label: string}>} options.columns 数据列定义（不含序号列）
 * @param {Array<string[]>} options.rows 行数据，单元格为已转义的 HTML 文本
 * @returns {Promise<HTMLElement|null>} 模态框根节点
 */
export async function openSimpleListModal({ title, columns, rows }) {
  const modal = await openModal("simple-list-modal");
  if (!modal) {
    return null;
  }

  const titleEl = modal.querySelector("#simple-list-modal-title");
  if (titleEl) {
    titleEl.textContent = title;
  }

  const theadTr = modal.querySelector("#simple-list-thead-tr");
  if (theadTr) {
    theadTr.innerHTML = `<th data-i18n="common.index">No.</th>${columns
      .map((col) => `<th>${col.label}</th>`)
      .join("")}`;
  }

  const tbody = modal.querySelector("#simple-list-tbody");
  if (tbody) {
    if (!rows || rows.length === 0) {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${columns.length + 1}" class="text-center">${t("common.no_data")}</td></tr>`;
    } else {
      tbody.innerHTML = rows
        .map(
          (cells, idx) =>
            `<tr><td class="index-column">${idx + 1}</td>${cells
              .map((cell) => `<td>${cell}</td>`)
              .join("")}</tr>`
        )
        .join("");
    }
  }

  return modal;
}
