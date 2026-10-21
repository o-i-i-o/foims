/**
 * 统一 API 信封与分页契约（手写，不随 ts-rs 生成）。
 *
 * - 后端 `ApiResponse<T>` 信封不导出：client 层归一化时附加
 *   errorType/isBlob/filename/message_key 字段，与 Rust 侧结构不同；
 * - `paged_response` 返回 serde_json::Value 无法 derive，`Paged<T>`
 *   五键契约（items/total/page/page_size/total_pages）以此处为准。
 * - 后端业务载荷类型在 `./generated/`（ts-rs 生成，只读禁手改）。
 */

/** 服务端消息键的动态插值参数（`{{name}}` 插值） */
export type MessageParams = Record<string, string>;

/**
 * 前端统一 API 结果：client 层永不 reject，成功与失败均归一为此结构。
 * success 为 true 时 data 携带业务载荷；为 false 时 message 为已翻译文案。
 */
export interface ApiResult<T> {
  success: boolean;
  /** 已就地翻译的服务端消息 */
  message: string;
  /** 服务端消息原始 i18n 键（调用方按键分支，如验证码联动） */
  message_key?: string;
  /** 失败类别：unauthorized / token_expired / network_error / api_error 等 */
  errorType?: string;
  /** 失败附加详情（如 network_error 的 originalError） */
  errorDetails?: Record<string, unknown>;
  /** 服务端建议操作（如忘记密码引导） */
  suggestedAction?: string;
  /** Blob 下载响应标记 */
  isBlob?: boolean;
  /** 下载文件名（Content-Disposition 解析） */
  filename?: string;
  /** 业务载荷；失败或无数据时为 null */
  data: T | null;
}

/** 统一分页响应（对应后端 paged_response 五键契约） */
export interface Paged<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}
