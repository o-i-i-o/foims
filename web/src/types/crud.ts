import type { ApiResponse } from "./api.js";

export interface CrudConfig<T> {
  endpoint: string;
  entityName: string;
  entityNameKey?: string;
  formId: string;
  modalId: string;
  loadListCallback?: (data: unknown) => void;
  renderRowCallback?: (item: T, index: number) => string;
  validateCallback?: (data: Record<string, unknown>) => string | null;
  transformDataCallback?: (data: Record<string, unknown>) => Record<string, unknown>;
  afterCreateCallback?: (data: T) => Promise<void> | void;
  afterUpdateCallback?: (data: T) => Promise<void> | void;
  afterDeleteCallback?: (id: number | string) => Promise<void> | void;
}

export interface CrudManager<T> {
  list(params?: CrudListParams): Promise<ApiResponse | null>;
  get(id: number | string): Promise<T | null>;
  create(
    data: Record<string, unknown>,
    options?: CrudOperationOptions
  ): Promise<CrudResult<T>>;
  update(
    id: number | string,
    data: Record<string, unknown>,
    options?: CrudOperationOptions
  ): Promise<CrudResult<T>>;
  delete(
    id: number | string,
    options?: CrudDeleteOptions
  ): Promise<CrudDeleteResult>;
  openModal(title?: string, data?: T | null): void;
  closeModal(): void;
  populateForm(form: HTMLFormElement, data: T): void;
  getFormData(): Record<string, unknown> | null;
  submitForm(editId?: number | string | null): Promise<CrudResult<T>>;
}

export interface CrudListParams {
  page?: number;
  pageSize?: number;
  search?: string;
  [key: string]: unknown;
}

export interface CrudOperationOptions {
  skipValidation?: boolean;
  silent?: boolean;
}

export interface CrudDeleteOptions {
  silent?: boolean;
  confirmMessage?: string | null;
}

export interface CrudResult<T> {
  success: boolean;
  data?: T;
  message?: string;
}

export interface CrudDeleteResult {
  success: boolean;
  cancelled?: boolean;
  message?: string;
}

export interface PaginatedLoaderOptions<T> {
  pageSize?: number;
  transform?: (item: unknown) => T;
}

export interface PaginatedResult<T> {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}
