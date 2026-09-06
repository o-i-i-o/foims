import { withVersion } from "./resourceLoader.js";

const loadedStyles = new Set();
const loadingStyles = new Map();

// dashboard.css / organization.css 已在 main.html 静态引入，无需动态加载；
// 仅 visualization.css 未静态引入，按页面按需加载。
const PAGE_STYLES = {
  resources: ["/static/css/pages/visualization.css"],
  visualization: ["/static/css/pages/visualization.css"]
};

async function loadStyle(href) {
  const url = withVersion(href);
  if (loadedStyles.has(url)) {
    return true;
  }

  if (loadingStyles.has(url)) {
    return loadingStyles.get(url);
  }

  const { promise, resolve, reject } = Promise.withResolvers();
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = url;

  link.onload = () => {
    loadedStyles.add(url);
    loadingStyles.delete(url);
    resolve(true);
  };

  link.onerror = () => {
    loadingStyles.delete(url);
    // 从 head 移除失败的 link，避免坏链接永久残留且每次重试再插入不断累积
    link.remove();
    console.error(`Failed to load style: ${url}`);
    reject(new Error(`Failed to load style: ${url}`));
  };

  document.head.appendChild(link);
  loadingStyles.set(url, promise);
  return promise;
}

export async function loadPageStyles(pageId) {
  const styles = PAGE_STYLES[pageId];
  if (!styles || styles.length === 0) {
    return;
  }

  const promises = styles.map((style) => loadStyle(style));
  await Promise.allSettled(promises);
}

export function preloadPageStyles(pageId) {
  const styles = PAGE_STYLES[pageId];
  if (!styles || styles.length === 0) {
    return;
  }

  styles.forEach((href) => {
    // 与 loadStyle 相同以版本化 URL 作为去重键，避免重复插入 prefetch
    const url = withVersion(href);
    if (!loadedStyles.has(url) && !loadingStyles.has(url)) {
      const link = document.createElement("link");
      link.rel = "prefetch";
      link.as = "style";
      link.href = url;
      document.head.appendChild(link);
    }
  });
}
