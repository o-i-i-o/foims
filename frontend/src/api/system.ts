/**
 * 系统设置域 API：配置段 / LDAP / SSO / 密码策略 / SMTP / 通知设置 /
 * fail2ban / 证书 / 导入导出 / systemd 服务 / 系统信息。
 *
 * 职责边界：只组装 URL 与请求体并返回 ApiResult，不做 toast / i18n / 状态管理。
 */
import { apiDelete, apiGet, apiPost, apiPut } from "./client";
import type { ApiResult } from "../types/api";
import type { AppFail2banStatus } from "../types/api/generated/AppFail2banStatus";
import type { BanIpRequest } from "../types/api/generated/BanIpRequest";
import type { CertificateInventory } from "../types/api/generated/CertificateInventory";
import type { GenerateCaRequest } from "../types/api/generated/GenerateCaRequest";
import type { GenerateCertRequest } from "../types/api/generated/GenerateCertRequest";
import type { LdapConfigResponse } from "../types/api/generated/LdapConfigResponse";
import type { NotificationSettings } from "../types/api/generated/NotificationSettings";
import type { PasswordPolicy } from "../types/api/generated/PasswordPolicy";
import type { SsoConfigResponse } from "../types/api/generated/SsoConfigResponse";
import type { SystemInfo } from "../types/api/generated/SystemInfo";
import type { UnbanIpRequest } from "../types/api/generated/UnbanIpRequest";
import type { UpdateFail2banConfigRequest } from "../types/api/generated/UpdateFail2banConfigRequest";
import type { UpdateLdapConfigRequest } from "../types/api/generated/UpdateLdapConfigRequest";
import type { UpdateSmtpConfigRequest } from "../types/api/generated/UpdateSmtpConfigRequest";
import type { UpdateSsoConfigRequest } from "../types/api/generated/UpdateSsoConfigRequest";

/** 封禁/解封响应载荷（后端 json! 动态构造，仅取提示文案） */
export interface BanActionResponse {
  message?: string;
}

/** SMTP 配置响应（后端未生成 ts-rs 类型；未配置属正常业务态） */
export interface SmtpConfigResponse {
  configured?: boolean;
  host?: string | null;
  port?: number | null;
  username?: string | null;
  from?: string | null;
  secure?: boolean;
  has_password?: boolean;
}

/** 单个 systemd 服务状态（后端未生成 ts-rs 类型） */
export interface ServiceStatusItem {
  name: string;
  unit: string;
  registered: boolean;
  active: boolean;
  active_state: string | null;
  sub_state: string | null;
  enabled: boolean;
  status_text?: string | null;
  reload_supported: boolean;
  uptime_seconds?: number | null;
}

/** 单表导入结果（后端 json! 动态构造，未生成 ts-rs 类型） */
export interface ImportResultRow {
  table?: string;
  inserted?: number;
  updated?: number;
}

/** CSV 导入响应载荷（后端 json! 动态构造，未生成 ts-rs 类型） */
export interface ImportCsvResponse {
  results?: ImportResultRow[] | null;
}

/** 证书类型：自生成（站点 CA 签发）/ 导入（外部证书） */
export type CertificateKind = "generated" | "imported";

// ==================== 系统配置（按段读写） ====================

/**
 * 读取系统配置原文：各卡片只消费自己的段（server/agent/snmp/rate_limit），
 * 载荷类型由调用方以泛型指定，避免为每段复制整份配置契约。
 */
export function getSystemConfig<T>(): Promise<ApiResult<T>> {
  return apiGet<T>("/api/system/config");
}

/** 提交系统配置局部段：后端按键合并，未携带的段保持原值 */
export function updateSystemConfig<B>(payload: B): Promise<ApiResult<unknown>> {
  return apiPut<unknown, B>("/api/system/config", payload);
}

/** 配置文件下载（attachment JSON，client 层归一为 Blob） */
export function backupSystemConfig(): Promise<ApiResult<unknown>> {
  return apiGet("/api/system/config/backup");
}

/** 提交整份配置恢复：字段校验与脱敏值还原均由服务端完成 */
export function restoreSystemConfig(config: unknown): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/config/restore", config);
}

// ==================== LDAP 认证 ====================

export function getLdapConfig(): Promise<ApiResult<LdapConfigResponse>> {
  return apiGet<LdapConfigResponse>("/api/system/ldap/config");
}

export function updateLdapConfig(payload: UpdateLdapConfigRequest): Promise<ApiResult<unknown>> {
  return apiPut<unknown, UpdateLdapConfigRequest>("/api/system/ldap/config", payload);
}

export function testLdapConnection(): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/ldap/test", {});
}

// ==================== SSO（OIDC） ====================

export function getSsoConfig(): Promise<ApiResult<SsoConfigResponse>> {
  return apiGet<SsoConfigResponse>("/api/system/sso/config");
}

export function updateSsoConfig(payload: UpdateSsoConfigRequest): Promise<ApiResult<unknown>> {
  return apiPut<unknown, UpdateSsoConfigRequest>("/api/system/sso/config", payload);
}

export function testSsoConnection(): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/sso/test", {});
}

// ==================== 等保密码策略 ====================

export function getPasswordPolicy(): Promise<ApiResult<PasswordPolicy>> {
  return apiGet<PasswordPolicy>("/api/system/password-policy");
}

export function updatePasswordPolicy(payload: PasswordPolicy): Promise<ApiResult<unknown>> {
  return apiPut<unknown, PasswordPolicy>("/api/system/password-policy", payload);
}

// ==================== SMTP 与通知设置 ====================

export function getSmtpConfig(): Promise<ApiResult<SmtpConfigResponse>> {
  return apiGet<SmtpConfigResponse>("/api/system/smtp/config");
}

export function updateSmtpConfig(payload: UpdateSmtpConfigRequest): Promise<ApiResult<unknown>> {
  return apiPut<unknown, UpdateSmtpConfigRequest>("/api/system/smtp/config", payload);
}

export function testSmtpConnection(): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/smtp/test", {});
}

export function getNotificationSettings(): Promise<ApiResult<NotificationSettings>> {
  return apiGet<NotificationSettings>("/api/system/notification/settings");
}

export function updateNotificationSettings(payload: NotificationSettings): Promise<ApiResult<unknown>> {
  return apiPut<unknown, NotificationSettings>("/api/system/notification/settings", payload);
}

// ==================== fail2ban ====================

export function getFail2banStatus(): Promise<ApiResult<AppFail2banStatus>> {
  return apiGet<AppFail2banStatus>("/api/system/fail2ban/app/status");
}

export function updateFail2banConfig(payload: UpdateFail2banConfigRequest): Promise<ApiResult<unknown>> {
  return apiPut<unknown, UpdateFail2banConfigRequest>("/api/system/fail2ban/app/config", payload);
}

export function banIp(ip: string): Promise<ApiResult<BanActionResponse>> {
  const payload: BanIpRequest = { ip };
  return apiPost<BanActionResponse, BanIpRequest>("/api/system/fail2ban/app/ban", payload);
}

export function unbanIp(ip: string): Promise<ApiResult<BanActionResponse>> {
  const payload: UnbanIpRequest = { ip };
  return apiPost<BanActionResponse, UnbanIpRequest>("/api/system/fail2ban/app/unban", payload);
}

// ==================== 证书库存与操作 ====================

/** 站点 CA 证书公开下载地址（浏览器直链） */
export const CA_DOWNLOAD_URL = "/api/certificate/ca/download";

export function getCertificateInventory(): Promise<ApiResult<CertificateInventory>> {
  return apiGet<CertificateInventory>("/api/system/certificate/list");
}

function certificatePath(kind: CertificateKind, fileStem: string): string {
  return `/api/system/certificate/${kind}/${encodeURIComponent(fileStem)}`;
}

/** 应用证书（自生成切换站点证书 / 导入证书部署） */
export function applyCertificate(kind: CertificateKind, fileStem: string): Promise<ApiResult<unknown>> {
  return apiPost(`${certificatePath(kind, fileStem)}/apply`);
}

export function deleteCertificate(kind: CertificateKind, fileStem: string): Promise<ApiResult<unknown>> {
  return apiDelete(certificatePath(kind, fileStem));
}

export function generateCa(payload: GenerateCaRequest): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/certificate/ca/generate", payload);
}

export function importCa(formData: FormData): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/certificate/ca/import", formData);
}

export function generateCertificate(payload: GenerateCertRequest): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/certificate/generate", payload);
}

export function importCertificate(formData: FormData): Promise<ApiResult<unknown>> {
  return apiPost("/api/system/certificate/import", formData);
}

// ==================== 导入导出 ====================

export function importCsvData(formData: FormData): Promise<ApiResult<ImportCsvResponse>> {
  return apiPost<ImportCsvResponse>("/api/system/import-export/import/csv", formData);
}

/** 导入模板（ZIP）下载 */
export function downloadImportTemplate(): Promise<ApiResult<unknown>> {
  return apiGet("/api/system/import-export/template?type=all");
}

/** 按模块导出 CSV 数据（ZIP，每张业务表一个 表名.csv） */
export function downloadCsvExport(moduleType: string): Promise<ApiResult<unknown>> {
  return apiGet(`/api/system/import-export/export/csv?type=${encodeURIComponent(moduleType)}`);
}

/** 整库导出（SQL 备份） */
export function downloadDatabaseExport(): Promise<ApiResult<unknown>> {
  return apiGet("/api/system/import-export/export/database");
}

// ==================== systemd 服务 ====================

export function getSystemServices(): Promise<ApiResult<ServiceStatusItem[]>> {
  return apiGet<ServiceStatusItem[]>("/api/system/services");
}

/** 服务操作：stop / restart / start / reload（后端先响应后延迟执行） */
export function runServiceOp(service: string, op: string): Promise<ApiResult<unknown>> {
  return apiPost(`/api/system/services/${encodeURIComponent(service)}/${encodeURIComponent(op)}`, {});
}

// ==================== 系统信息 ====================

export function getSystemInfo(): Promise<ApiResult<SystemInfo>> {
  return apiGet<SystemInfo>("/api/system/info");
}
