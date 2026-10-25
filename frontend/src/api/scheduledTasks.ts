/**
 * 定时任务域 API：任务列表 / 详请 / 增改 / 启停 / 立即执行 / 执行日志。
 *
 * 职责边界：只组装 URL 与请求体并返回 ApiResult，不做 toast / i18n / 状态管理。
 */
import { apiDelete, apiGet, apiPost, apiPut } from "./client";
import type { ApiResult, Paged } from "../types/api";
import type { ScheduledTask } from "../types/api/generated/ScheduledTask";
import type { ScheduledTaskCreate } from "../types/api/generated/ScheduledTaskCreate";
import type { TaskLog } from "../types/api/generated/TaskLog";

/** 任务立即执行响应载荷（后端 json! 动态构造，未生成 ts-rs 类型） */
export interface TaskRunResponse {
  result?: { ok?: string; err?: string } | null;
}

/** 任务列表地址（无分页，仅服务端排序） */
export function scheduledTasksUrl(sortBy: string, sortOrder: string): string {
  return `/api/system/scheduled-tasks?sort_by=${sortBy}&sort_order=${sortOrder}`;
}

/** 任务列表（无分页，仅服务端排序） */
export function listScheduledTasks(sortBy: string, sortOrder: string): Promise<ApiResult<ScheduledTask[]>> {
  return apiGet<ScheduledTask[]>(scheduledTasksUrl(sortBy, sortOrder));
}

/** 单任务执行日志（固定取最近 50 条，五键分页响应） */
export function scheduledTaskLogsUrl(taskName: string): string {
  return `/api/system/scheduled-tasks/logs?task_name=${encodeURIComponent(taskName)}&page=1&page_size=50`;
}

export function getScheduledTask(id: string): Promise<ApiResult<ScheduledTask>> {
  return apiGet<ScheduledTask>(`/api/system/scheduled-tasks/${id}`);
}

export function createScheduledTask(payload: ScheduledTaskCreate): Promise<ApiResult<ScheduledTask>> {
  return apiPost<ScheduledTask, ScheduledTaskCreate>("/api/system/scheduled-tasks", payload);
}

export function updateScheduledTask(id: string, payload: ScheduledTaskCreate): Promise<ApiResult<ScheduledTask>> {
  return apiPut<ScheduledTask, ScheduledTaskCreate>(`/api/system/scheduled-tasks/${id}`, payload);
}

export function runScheduledTask(id: string): Promise<ApiResult<TaskRunResponse>> {
  return apiPost<TaskRunResponse>(`/api/system/scheduled-tasks/${id}/run`);
}

export function toggleScheduledTask(id: string): Promise<ApiResult<unknown>> {
  return apiPost(`/api/system/scheduled-tasks/${id}/toggle`);
}

export function deleteScheduledTask(id: string): Promise<ApiResult<unknown>> {
  return apiDelete(`/api/system/scheduled-tasks/${id}`);
}

/** 执行日志查询（TaskLogsModal 复用同一个分页载荷） */
export function getScheduledTaskLogs(taskName: string): Promise<ApiResult<Paged<TaskLog>>> {
  return apiGet<Paged<TaskLog>>(scheduledTaskLogsUrl(taskName));
}
