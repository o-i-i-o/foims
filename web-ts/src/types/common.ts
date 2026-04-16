export type SortOrder = "asc" | "desc";

export interface SortState {
  by: string;
  order: SortOrder;
  toggle(key: string): SortState;
  setSort(by: string, order?: SortOrder | null): SortState;
  readonly sortBy: string;
  readonly sortOrder: SortOrder;
}

export interface TableState {
  readonly isLoading: boolean;
  readonly currentPage: number;
  readonly sortBy: string;
  readonly sortOrder: SortOrder;
  readonly pageSize: number;
  readonly sortState: SortState;
  setLoading(value: boolean): void;
  setPage(page: number): TableState;
  setSort(by: string, order: SortOrder): TableState;
  toggleSort(key: string): TableState;
  getQueryParams(): Record<string, string | number>;
}

export interface PaginationState {
  getPage(): number;
  getTotal(): number;
  getTotalPages(): number;
  getPageSize(): number;
  getOffset(): number;
  setPage(page: number): PaginationState;
  setTotal(total: number): PaginationState;
  next(): boolean;
  prev(): boolean;
  first(): PaginationState;
  last(): PaginationState;
  getQueryParams(): Record<string, number>;
}

export type ToastType = "success" | "error" | "warning" | "info";

export interface ConfirmOptions {
  title?: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
  message?: string;
}

export interface ColumnDef<T> {
  field: keyof T & string;
  render?: (value: T[keyof T], row: T, index: number) => string | Node | undefined | null;
  className?: string;
}

export interface TableOptions<T> {
  data: T[];
  columns: ColumnDef<T>[];
  emptyMessage?: string;
  rowIdField?: keyof T & string;
  onRowClick?: (row: T, index: number) => void;
  onRowDoubleClick?: (row: T, index: number) => void;
}
