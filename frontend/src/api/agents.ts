/**
 * Agent 域 API：注册令牌 / 配置包下载。
 *
 * 职责边界：只组装 URL 与请求体并返回 ApiResult，不做 toast / i18n / 状态管理。
 */
import { apiGet, apiPost } from "./client";
import type { ApiResult } from "../types/api";
import type { EnrollmentTokenResponse } from "../types/api/generated/EnrollmentTokenResponse";

/** 当前注册令牌（首次访问不存在时由后端生成） */
export function getEnrollmentToken(): Promise<ApiResult<EnrollmentTokenResponse>> {
  return apiGet<EnrollmentTokenResponse>("/api/agents/enrollment-token");
}

/** 重新生成注册令牌：旧令牌即刻失效，已部署 agent 需重新下发配置包 */
export function regenerateEnrollmentToken(): Promise<ApiResult<EnrollmentTokenResponse>> {
  return apiPost<EnrollmentTokenResponse>("/api/agents/enrollment-token", {});
}

/** 配置包下载（agent.toml 与证书由服务端现场组包，client 层归一为 Blob） */
export function downloadAgentConfigPackage(format: string): Promise<ApiResult<unknown>> {
  return apiGet(`/api/agents/config-package?format=${encodeURIComponent(format)}`);
}
