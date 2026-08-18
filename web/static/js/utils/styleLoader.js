import { MODULE_VERSION } from "./resourceLoader.js";

const loadedStyles = new Set();
const loadingStyles = new Map();

// dashboard.css / organization.css 已在 main.html 静态引入，无需动态加载；
// 仅 visualization.css 未静态引入，按页面按需加载。
const PAGE_STYLES = {
  resources: ["/static/css/pages/visualization.css"],
  visualization: ["/static/css/pages/visualization.css"]
};

// 拼接资源版本号，与 main.html 静态资源的 ?v= 缓存穿透机制保持一致
function withVersion(href) {
  return href.includes("?") ? `${href}&v=${MODULE_VERSION}` : `${href}?v=${MODULE_VERSION}`;
}

async function loadStyle(href) {
  const url = withVersion(href);
  if (loadedStyles.has(url)) {
    return true;
  }

  if (loadingStyles.has(url)) {
    return loadingStyles.get(url);
  }

  const promise = new Promise((resolve, reject) => {
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
      console.error(`Failed to load style: ${url}`);
      reject(new Error(`Failed to load style: ${url}`));
    };

    document.head.appendChild(link);
  });

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
    if (!loadedStyles.has(href) && !loadingStyles.has(href)) {
      const link = document.createElement("link");
      link.rel = "prefetch";
      link.as = "style";
      link.href = href;
      document.head.appendChild(link);
    }
  });
}
