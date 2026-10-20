import { t } from "./legacyBridge.js";

/**
 * 可视化数据加载的共享工具：SVG 布局与拓扑两个数据管理器复用，
 * 避免同一套分页拉取/失败提示逻辑各自维护副本。
 */

/** 数据获取失败时的统一提示（请求异常或业务失败） */
export function notifyLoadFailure(showToast, error, what) {
  console.error(`${what}失败:`, error);
  showToast(t("viz.data_load_failed"), "error");
}

/**
 * 分页拉取全量列表：每页 1000 条，最多 5 页（5000 条）。
 * 仍有后续页但已达上限时提示数据可能不完整（避免超 1000 条被静默截断）。
 * @param {Object} deps
 * @param {Function} deps.apiGet API GET 封装（apiClient.apiGet 或实例绑定）
 * @param {Function} deps.showToast 通知函数
 * @param {string} deps.path 不含分页参数的接口路径
 * @param {string} deps.what 失败提示用途描述
 * @returns {Promise<Array>} 拉取到的条目（失败时为已获取的部分或空数组）
 */
export async function fetchAllPages({ apiGet, showToast, path, what }) {
  const MAX_PAGES = 5;
  const PAGE_SIZE = 1000;
  const sep = path.includes("?") ? "&" : "?";

  // 递归逐页拉取：失败与到达末页需短路终止（保留顺序拉取语义），
  // 以递归替代 for+await（no-await-in-loop）；页数上限 5，递归深度有界
  const fetchFrom = async (page, items) => {
    let result;
    try {
      result = await apiGet(`${path}${sep}page=${page}&page_size=${PAGE_SIZE}`);
    } catch (error) {
      notifyLoadFailure(showToast, error, what);
      return items;
    }
    if (!result.success || !Array.isArray(result.data?.items)) {
      if (!result.success) {
        notifyLoadFailure(showToast, result.message, what);
      }
      return items;
    }

    items.push(...result.data.items);

    const totalPages = Number(result.data.total_pages) || 1;
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
