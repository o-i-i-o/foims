import type { CrudConfig, CrudManager, CrudResult, CrudDeleteResult, CrudListParams, CrudOperationOptions, CrudDeleteOptions, PaginatedLoaderOptions, PaginatedResult } from "../types/crud.js";
import { apiGet, apiPost, apiPut, apiDelete } from "./apiClient.js";
import type { ApiResponse } from "../types/api.js";
import { showToast } from "./toast.js";
import { showConfirm } from "./confirm.js";
import { t } from "./i18n.js";

const DEFAULT_PAGE_SIZE = 20;

export function createCrudManager<T>(config: CrudConfig<T>): CrudManager<T> {
  const {
    endpoint,
    entityName,
    entityNameKey,
    loadListCallback,
    formId,
    modalId,
    validateCallback,
    transformDataCallback,
    afterCreateCallback,
    afterUpdateCallback,
    afterDeleteCallback,
  } = config;

  const nameKey = entityNameKey || entityName;

  return {
    async list(params: CrudListParams = {}): Promise<ApiResponse | null> {
      const { page = 1, pageSize = DEFAULT_PAGE_SIZE, search = "", ...otherParams } = params;
      const queryParams = new URLSearchParams({
        page: page.toString(),
        page_size: pageSize.toString(),
        ...(search && { search }),
        ...Object.entries(otherParams).reduce<Record<string, string>>((acc, [key, value]) => {
          if (value !== undefined && value !== null && value !== "") {
            acc[key] = String(value);
          }
          return acc;
        }, {}),
      });

      const result = await apiGet(`${endpoint}?${queryParams}`);

      if (result.success) {
        if (loadListCallback) {
          loadListCallback(result.data);
        }
        return result;
      } else {
        showToast(result.message || t("common.load_failed"), "error");
        return null;
      }
    },

    async get(id: number | string): Promise<T | null> {
      const result = await apiGet<T>(`${endpoint}/${id}`);
      if (!result.success) {
        showToast(result.message || t("common.load_failed"), "error");
        return null;
      }
      return result.data ?? null;
    },

    async create(data: Record<string, unknown>, options: CrudOperationOptions = {}): Promise<CrudResult<T>> {
      const { skipValidation = false, silent = false } = options;

      if (validateCallback && !skipValidation) {
        const validationError = validateCallback(data);
        if (validationError) {
          if (!silent) showToast(validationError, "error");
          return { success: false, message: validationError };
        }
      }

      const requestData = transformDataCallback ? transformDataCallback(data) : data;
      const result = await apiPost<T>(endpoint, requestData);

      if (result.success) {
        if (!silent) showToast(t("common.create_success", { name: t(nameKey) }), "success");
        if (afterCreateCallback && result.data) await afterCreateCallback(result.data);
        return { success: true, data: result.data };
      } else {
        if (!silent) showToast(result.message || t("common.create_failed", { name: t(nameKey) }), "error");
        return { success: false, message: result.message };
      }
    },

    async update(id: number | string, data: Record<string, unknown>, options: CrudOperationOptions = {}): Promise<CrudResult<T>> {
      const { skipValidation = false, silent = false } = options;

      if (validateCallback && !skipValidation) {
        const validationError = validateCallback(data);
        if (validationError) {
          if (!silent) showToast(validationError, "error");
          return { success: false, message: validationError };
        }
      }

      const requestData = transformDataCallback ? transformDataCallback(data) : data;
      const result = await apiPut<T>(`${endpoint}/${id}`, requestData);

      if (result.success) {
        if (!silent) showToast(t("common.update_success", { name: t(nameKey) }), "success");
        if (afterUpdateCallback && result.data) await afterUpdateCallback(result.data);
        return { success: true, data: result.data };
      } else {
        if (!silent) showToast(result.message || t("common.update_failed", { name: t(nameKey) }), "error");
        return { success: false, message: result.message };
      }
    },

    async delete(id: number | string, options: CrudDeleteOptions = {}): Promise<CrudDeleteResult> {
      const { silent = false, confirmMessage = null } = options;

      if (confirmMessage) {
        const confirmed = await showConfirm(confirmMessage);
        if (!confirmed) return { success: false, cancelled: true };
      }

      const result = await apiDelete(`${endpoint}/${id}`);

      if (result.success) {
        if (!silent) showToast(t("common.delete_success", { name: t(nameKey) }), "success");
        if (afterDeleteCallback) await afterDeleteCallback(id);
        return { success: true };
      } else {
        if (!silent) showToast(result.message || t("common.delete_failed", { name: t(nameKey) }), "error");
        return { success: false, message: result.message };
      }
    },

    openModal(title = "", data: T | null = null): void {
      const modal = document.getElementById(modalId);
      if (!modal) return;

      const titleEl = modal.querySelector(".modal-title") as HTMLElement | null;
      if (titleEl) titleEl.textContent = title || t(`${nameKey}.add`);

      const form = document.getElementById(formId) as HTMLFormElement | null;
      if (form) {
        form.reset();
        if (data) {
          this.populateForm(form, data);
        }
      }

      modal.classList.add("active");
      document.body.style.overflow = "hidden";
    },

    closeModal(): void {
      const modal = document.getElementById(modalId);
      if (!modal) return;

      modal.classList.remove("active");
      document.body.style.overflow = "";

      const form = document.getElementById(formId) as HTMLFormElement | null;
      if (form) form.reset();
    },

    populateForm(form: HTMLFormElement, data: T): void {
      Object.entries(data as Record<string, unknown>).forEach(([key, value]) => {
        const field = form.elements.namedItem(key) as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement | null || form.querySelector(`[name="${key}"]`) as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement | null;
        if (field) {
          if ((field as HTMLInputElement).type === "checkbox") {
            (field as HTMLInputElement).checked = !!value;
          } else {
            (field as HTMLInputElement).value = value != null ? String(value) : "";
          }
        }
      });
    },

    getFormData(): Record<string, unknown> | null {
      const form = document.getElementById(formId) as HTMLFormElement | null;
      if (!form) return null;

      const formData = new FormData(form);
      const data: Record<string, unknown> = {};
      formData.forEach((value, key) => {
        const field = form.elements.namedItem(key) as HTMLInputElement | null;
        if (field && field.type === "checkbox") {
          data[key] = field.checked;
        } else {
          data[key] = value;
        }
      });
      return data;
    },

    async submitForm(editId: number | string | null = null): Promise<CrudResult<T>> {
      const data = this.getFormData();
      if (!data) return { success: false, message: "Form not found" };

      if (editId) {
        return await this.update(editId, data);
      } else {
        return await this.create(data);
      }
    },
  };
}

export function createPaginatedLoader<T>(endpoint: string, options: PaginatedLoaderOptions<T> = {}): {
  load(page?: number, search?: string, extraParams?: Record<string, unknown>): Promise<PaginatedResult<T>>;
  next(extraParams?: Record<string, unknown>): Promise<PaginatedResult<T> | null>;
  prev(extraParams?: Record<string, unknown>): Promise<PaginatedResult<T> | null>;
  getPage(): number;
  getTotal(): number;
  getTotalPages(): number;
} {
  const { pageSize = DEFAULT_PAGE_SIZE, transform = null } = options;
  let currentPage = 1;
  let totalCount = 0;
  let lastSearch = "";

  return {
    async load(page = 1, search = "", extraParams: Record<string, unknown> = {}): Promise<PaginatedResult<T>> {
      currentPage = page;
      lastSearch = search;

      const queryParams = new URLSearchParams({
        page: page.toString(),
        page_size: pageSize.toString(),
        ...(search && { search }),
        ...Object.entries(extraParams).reduce<Record<string, string>>((acc, [key, value]) => {
          if (value !== undefined && value !== null && value !== "") {
            acc[key] = String(value);
          }
          return acc;
        }, {}),
      });

      const result = await apiGet(`${endpoint}?${queryParams}`);

      if (result.success) {
        const data = result.data as Record<string, unknown> | undefined;
        totalCount = (data?.total as number) || 0;

        let items: T[] = (data?.items as T[]) || (Array.isArray(data) ? data as T[] : []);
        if (transform) {
          items = items.map(transform) as T[];
        }

        return {
          items,
          total: totalCount,
          page: currentPage,
          pageSize,
          totalPages: Math.ceil(totalCount / pageSize),
        };
      }

      return { items: [], total: 0, page: 1, pageSize, totalPages: 0 };
    },

    async next(extraParams: Record<string, unknown> = {}): Promise<PaginatedResult<T> | null> {
      const totalPages = Math.ceil(totalCount / pageSize);
      if (currentPage < totalPages) {
        return await this.load(currentPage + 1, lastSearch, extraParams);
      }
      return null;
    },

    async prev(extraParams: Record<string, unknown> = {}): Promise<PaginatedResult<T> | null> {
      if (currentPage > 1) {
        return await this.load(currentPage - 1, lastSearch, extraParams);
      }
      return null;
    },

    getPage(): number { return currentPage; },
    getTotal(): number { return totalCount; },
    getTotalPages(): number { return Math.ceil(totalCount / pageSize); },
  };
}
