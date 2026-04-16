declare global {
  interface Window {
    openUserModal: (userId?: string | null) => void;
    openTwoFactorModal: (userId: string, username: string, isEnabled: boolean) => Promise<void>;
    IPMA_CONFIG?: {
      apiBaseUrl?: string;
      language?: string;
      debug?: boolean;
    };
  }

  interface HTMLElement {
    dataAttributes: Record<string, string>;
  }

  type AsyncFunction<T = void> = () => Promise<T>;
  type SyncFunction<T = void> = () => T;
  type AnyFunction<T = void> = AsyncFunction<T> | SyncFunction<T>;

  interface PaginatedResult<T> {
    items: T[];
    total: number;
    page: number;
    pageSize: number;
    totalPages: number;
  }

  interface SelectOption {
    value: string | number;
    label: string;
    disabled?: boolean;
  }

  interface TableColumn<T = Record<string, unknown>> {
    field: keyof T & string;
    header?: string;
    render?: (value: T[keyof T], row: T, index: number) => string | HTMLElement | undefined | null;
    className?: string;
    sortable?: boolean;
    width?: string;
  }

  interface FormField {
    name: string;
    label: string;
    type: "text" | "number" | "email" | "password" | "select" | "textarea" | "checkbox";
    required?: boolean;
    placeholder?: string;
    options?: SelectOption[];
    defaultValue?: string | number | boolean;
    validation?: (value: unknown) => string | null;
  }

  interface ApiResponse<T = unknown> {
    success: boolean;
    data?: T;
    message?: string;
    errorType?: string;
    errorDetails?: Record<string, unknown>;
    suggestedAction?: string;
  }

  interface PagedData<T> {
    items: T[];
    total: number;
    page: number;
    page_size: number;
  }
}

export {};
