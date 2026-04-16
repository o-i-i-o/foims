export interface ApiResponse<T = unknown> {
  success: boolean;
  data?: T;
  message?: string;
  errorType?: string;
  errorDetails?: Record<string, unknown>;
  suggestedAction?: string;
}

export interface ApiErrorResponse {
  success: false;
  message: string;
  errorType: string;
  errorDetails?: Record<string, unknown>;
  suggestedAction?: string;
}

export interface BlobResponse {
  success: true;
  data: Blob;
  isBlob: true;
  filename: string;
}

export interface PagedData<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
}

export type ApiResult<T = unknown> = ApiResponse<T> | ApiErrorResponse | BlobResponse;

export function extractItems<T>(result: ApiResponse<T[] | PagedData<T>>): T[] {
  if (!result.success || !result.data) return [];
  if (Array.isArray(result.data)) return result.data;
  if ("items" in result.data && Array.isArray(result.data.items))
    return result.data.items;
  return [];
}
