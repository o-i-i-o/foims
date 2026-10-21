import { t } from "./legacyBridge.js";
import type { ApiResult } from "../types/api";

/**
 * 可视化数据加载的共享工具：SVG 布局与拓扑两个数据管理器复用，
 * 避免同一套分页拉取/失败提示逻辑各自维护副本。
 */

/** 通知函数最小签名（message, type），兼容 useToast.showToast 与旧版桥接实现 */
type ShowToastFn = (message: string, type?: string) => void;

/** 分页载荷宽松形状：total_pages 允许非数值，由 Number() 兜底 */
interface PagedPayload {
  items?: unknown;
  total_pages?: unknown;
}

/** fetchAllPages 依赖注入 */
interface FetchAllPagesDeps {
  /** API GET 封装（apiClient.apiGet 或实例绑定） */
  apiGet: (url: string) => Promise<ApiResult<unknown>>;
  /** 通知函数 */
  showToast: ShowToastFn;
  /** 不含分页参数的接口路径 */
  path: string;
  /** 失败提示用途描述 */
  what: string;
}

/** 数据获取失败时的统一提示（请求异常或业务失败） */
export function notifyLoadFailure(showToast: ShowToastFn, error: unknown, what: string): void {
  console.error(`${what}失败:`, error);
  showToast(t("viz.data_load_failed"), "error");
}

/**
 * 分页拉取全量列表：每页 1000 条，最多 5 页（5000 条）。
 * 仍有后续页但已达上限时提示数据可能不完整（避免超 1000 条被静默截断）。
 * @returns 拉取到的条目（失败时为已获取的部分或空数组）
 */
export async function fetchAllPages({ apiGet, showToast, path, what }: FetchAllPagesDeps): Promise<unknown[]> {
  const MAX_PAGES = 5;
  const PAGE_SIZE = 1000;
  const sep = path.includes("?") ? "&" : "?";

  // 递归逐页拉取：失败与到达末页需短路终止（保留顺序拉取语义），
  // 以递归替代 for+await（no-await-in-loop）；页数上限 5，递归深度有界
  const fetchFrom = async (page: number, items: unknown[]): Promise<unknown[]> => {
    let result: ApiResult<PagedPayload>;
    try {
      // client 层归一化永不 reject，catch 仅作防御（实例绑定实现的意外抛出）
      result = (await apiGet(`${path}${sep}page=${page}&page_size=${PAGE_SIZE}`)) as ApiResult<PagedPayload>;
    } catch (error) {
      notifyLoadFailure(showToast, error, what);
      return items;
    }
    const pageItems = result.data?.items;
    if (!result.success || !Array.isArray(pageItems)) {
      if (!result.success) {
        notifyLoadFailure(showToast, result.message, what);
      }
      return items;
    }

    items.push(...pageItems);

    const totalPages = Number(result.data?.total_pages) || 1;
    if (page >= totalPages) {
      return items;
    }
    if (page === MAX_PAGES) {
      // 还有后续页但已达拉取上限：提示数据可能不完整
      showToast(t("viz.data_page_limit_exceeded"), "warning");
      return items;
    }
    return fetchFrom(page + 1, items);
  };

  return fetchFrom(1, []);
}
