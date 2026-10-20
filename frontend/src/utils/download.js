/**
 * 文件下载工具：触发浏览器保存 apiClient 返回的 blob 结果。
 * apiClient 将下载响应归一为 { success, data: Blob, isBlob: true, filename }。
 */

// 触发浏览器下载 apiClient 返回的 blob 结果；非 blob 结果直接忽略（调用方负责提示错误）
export function downloadBlobResult(result, fallbackName) {
  if (!result.isBlob) {
    return;
  }
  const url = window.URL.createObjectURL(result.data);
  const a = document.createElement("a");
  a.href = url;
  a.download = result.filename && result.filename !== "download" ? result.filename : fallbackName;
  document.body.appendChild(a);
  a.click();
  window.URL.revokeObjectURL(url);
  document.body.removeChild(a);
}
