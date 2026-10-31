/**
 * 日志 / 通知域的 API 封装（纯传输层）。
 *
 * 职责边界与 `visualization.ts` 一致：只负责「组装 URL / 请求体 → 返回
 * `ApiResult`」，不做通知、不做 i18n、不做状态管理；client 层永不 reject，
 * 因此这里也不再包 try/catch。
 *
 * 路径集中在模块内（页面不再拼 `/api/...` 字面量），后端改动只需改这一处；
 * 对应后端 logs.rs / notifications.rs 与 src/system/logs*.rs。
 */
import { apiGet, apiPost, apiPut } from "./client";
import type { ApiResult } from "../types/api";
import type { ClearLogsRequest } from "../types/api/generated/ClearLogsRequest";

/** 分页查询参数（与 usePagedList 的 buildUrl 实参一一对应） */
export interface PagedQuery {
  page: number;
  pageSize: number;
  sortBy: string;
  sortOrder: string;
}

/** 单类日志统计（后端 json! 动态构造，未生成 ts-rs 类型） */
export interface LogCategoryStats {
  count?: number;
  oldest?: string | null;
}

/** GET /api/system/logs/stats 响应 */
export interface LogsStats {
  operation_logs?: LogCategoryStats | null;
  login_logs?: LogCategoryStats | null;
  notifications?: LogCategoryStats | null;
}

/** GET 返回 / PUT 提交同形的日志外发（syslog）配置 */
export interface LogForwarding {
  enabled?: boolean;
  protocol?: string | null;
  host?: string | null;
  port?: number | null;
}

/** 分页 URL 公共拼装（页码/页大小/排序三件套进查询串） */
function buildPagedUrl(path: string, query: PagedQuery, extra?: (params: URLSearchParams) => void): string {
  const params = new URLSearchParams({
    page: String(query.page),
    page_size: String(query.pageSize),
    sort_by: query.sortBy,
    sort_order: query.sortOrder
  });
  extra?.(params);
  return `${path}?${params}`;
}

/** 操作日志分页 URL（action 为操作类型过滤，空值不带该参数） */
export function operationLogsUrl(query: PagedQuery, action: string): string {
  return buildPagedUrl("/api/logs/operation", query, (params) => {
    if (action) {
      params.set("action", action);
    }
  });
}

/** 登录日志分页 URL */
export function loginLogsUrl(query: PagedQuery): string {
  return buildPagedUrl("/api/logs/login", query);
}

/** 运行日志 SSE 流 URL（EventSource 建连用，lang 为期望的日志语言） */
export function runtimeLogsUrl(lang: string): string {
  return `/api/logs/live?lang=${encodeURIComponent(lang)}`;
}

/** 通知列表分页 URL */
export function notificationsUrl(query: PagedQuery): string {
  return buildPagedUrl("/api/notifications", query);
}

/** 单条通知标记已读 */
export function markNotificationRead(id: string): Promise<ApiResult<unknown>> {
  return apiPut(`/api/notifications/${id}/read`, {});
}

/** 全部通知标记已读 */
export function markAllNotificationsRead(): Promise<ApiResult<unknown>> {
  return apiPut("/api/notifications/mark-all-read", {});
}

/** 日志统计（各类别条数与最早记录时间） */
export function getLogStats(): Promise<ApiResult<LogsStats>> {
  return apiGet<LogsStats>("/api/system/logs/stats");
}

/** 按日志类型清理（days=0 表示清空该类型全部日志） */
export function clearLogs(payload: ClearLogsRequest): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/logs/clear", payload);
}

/** 读取日志外发配置 */
export function getLogForwarding(): Promise<ApiResult<LogForwarding>> {
  return apiGet<LogForwarding>("/api/system/logs/forwarding");
}

/** 保存日志外发配置 */
export function saveLogForwarding(payload: LogForwarding): Promise<ApiResult<unknown>> {
  return apiPut("/api/system/logs/forwarding", payload);
}

/** 发送测试报文验证 syslog 目的地可达 */
export function testLogForwarding(): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/logs/forwarding/test", {});
}
