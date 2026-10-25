/**
 * 拓扑设备类型图标（本地 lucide 图标库 `/icons/lucide/`）。
 *
 * lucide 发布产物结构固定：license 注释 + 单根 `<svg>`，线稿子元素不带描边
 * 属性、依赖根节点约定。这里剥掉根标签只留内部标记，由画布自带的 32×32
 * 图标骨架补齐描边，颜色经 CSS `currentColor` 按设备类型供给。
 */
const DEVICE_ICON_FILES: Record<string, string> = {
  switch: "network",
  network_device: "cpu",
  server: "server",
  router: "router",
  camera: "cctv",
  phone: "phone",
  desktop: "monitor",
  laptop: "laptop",
  printer: "printer",
  other: "package",
};

const LUCIDE_ICON_BASE = "/icons/lucide/";
const FALLBACK_ICON_FILE = "package";

/** 图标文件 Promise 缓存：同一图标跨重绘只取一次；失败不缓存负结果，下次自动重试 */
const markupCache = new Map<string, Promise<string>>();

export function deviceIconFile(type: string | null): string {
  return (type ? DEVICE_ICON_FILES[type] : undefined) ?? FALLBACK_ICON_FILE;
}

export function loadDeviceIconMarkup(type: string | null): Promise<string> {
  const file = deviceIconFile(type);
  const cached = markupCache.get(file);
  if (cached) {
    return cached;
  }
  const pending = fetch(`${LUCIDE_ICON_BASE}${file}.svg`)
    .then((response) => {
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      return response.text();
    })
    .then((text) => {
      const start = text.indexOf("<svg");
      const openEnd = start >= 0 ? text.indexOf(">", start) : -1;
      const close = text.lastIndexOf("</svg>");
      if (start < 0 || openEnd < 0 || close < 0 || close < openEnd) {
        throw new Error("unexpected lucide svg structure");
      }
      return text.slice(openEnd + 1, close);
    })
    .catch((error: unknown) => {
      console.error(`加载 lucide 设备图标失败: ${file}.svg`, error);
      markupCache.delete(file);
      return "";
    });
  markupCache.set(file, pending);
  return pending;
}
