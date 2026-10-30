/**
 * 可视化数据加载的共享工具：布局画布与拓扑画布复用同一套分页拉取/
 * 失败提示逻辑，避免各自维护副本。
 *
 * 依赖以参数注入（`apiGet` / `showToast`），本模块不持有任何全局单例，
 * 便于在 composable 层与自检脚本中替换。
 */
import { i18n } from "../i18n/index";
import type { ToastType } from "../composables/useToast";
import type { ApiResult, Paged } from "../types/api";

/** 通知函数最小签名，兼容 useToast.showToast（多余参数可选即可赋值） */
export type ShowToastFn = (message: string, type?: ToastType) => void;

/** 分页拉取上限：每页 1000 条 × 5 页 */
const MAX_PAGES = 5;
const PAGE_SIZE = 1000;

/** 数据获取失败时的统一提示（请求异常或业务失败） */
export function notifyLoadFailure(showToast: ShowToastFn, error: unknown, what: string): void {
  console.error(`${what}失败:`, error);
  showToast(i18n.global.t("viz.data_load_failed"), "error");
}

/** 分页拉取的依赖注入 */
export interface FetchAllPagesDeps<T> {
  /** API GET 封装（`api/client` 的同名导出） */
  apiGet: (url: string) => Promise<ApiResult<Paged<T>>>;
  showToast: ShowToastFn;
  /** 不含分页参数的接口路径 */
  path: string;
  /** 失败提示中的用途描述（中文） */
  what: string;
}

/**
 * 分页拉取全量列表：每页 1000 条，最多 5 页（5000 条）。
 * 仍有后续页但已达上限时提示数据可能不完整（避免超 1000 条被静默截断）。
 * @returns 拉取到的条目（失败时为已获取的部分或空数组）
 */
export async function fetchAllPages<T>({ apiGet, showToast, path, what }: FetchAllPagesDeps<T>): Promise<T[]> {
  const sep = path.includes("?") ? "&" : "?";

  // 递归逐页拉取：失败与到达末页需短路终止（保留顺序拉取语义），
  // 以递归替代 for+await；页数上限 5，递归深度有界
  const fetchFrom = async (page: number, items: T[]): Promise<T[]> => {
    const result = await apiGet(`${path}${sep}page=${page}&page_size=${PAGE_SIZE}`);
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
      showToast(i18n.global.t("viz.data_page_limit_exceeded"), "warning");
      return items;
    }
    return fetchFrom(page + 1, items);
  };

  return fetchFrom(1, []);
}
