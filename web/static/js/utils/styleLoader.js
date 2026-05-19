const loadedStyles = new Set();
const loadingStyles = new Map();

const PAGE_STYLES = {
  dashboard: ['/static/css/pages/dashboard.css'],
  resources: ['/static/css/pages/dashboard.css'],
  ip: ['/static/css/pages/dashboard.css'],
  visualization: ['/static/css/pages/visualization.css'],
  logs: ['/static/css/pages/dashboard.css'],
  system: ['/static/css/pages/dashboard.css']
};

export function isStyleLoaded(href) {
  return loadedStyles.has(href);
}

export async function loadStyle(href) {
  if (loadedStyles.has(href)) {
    return true;
  }
  
  if (loadingStyles.has(href)) {
    return loadingStyles.get(href);
  }
  
  const promise = new Promise((resolve, reject) => {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = href;
    
    link.onload = () => {
      loadedStyles.add(href);
      loadingStyles.delete(href);
      resolve(true);
    };
    
    link.onerror = () => {
      loadingStyles.delete(href);
      console.error(`Failed to load style: ${href}`);
      reject(new Error(`Failed to load style: ${href}`));
    };
    
    document.head.appendChild(link);
  });
  
  loadingStyles.set(href, promise);
  return promise;
}

export async function loadPageStyles(pageId) {
  const styles = PAGE_STYLES[pageId];
  if (!styles || styles.length === 0) {
    return;
  }
  
  const promises = styles.map(style => loadStyle(style));
  await Promise.allSettled(promises);
}

export function preloadPageStyles(pageId) {
  const styles = PAGE_STYLES[pageId];
  if (!styles || styles.length === 0) {
    return;
  }
  
  styles.forEach(href => {
    if (!loadedStyles.has(href) && !loadingStyles.has(href)) {
      const link = document.createElement('link');
      link.rel = 'prefetch';
      link.as = 'style';
      link.href = href;
      document.head.appendChild(link);
    }
  });
}

export function unloadStyle(href) {
  const links = document.querySelectorAll(`link[rel="stylesheet"][href="${href}"]`);
  links.forEach(link => link.remove());
  loadedStyles.delete(href);
}

export function getLoadedStyles() {
  return new Set(loadedStyles);
}

export default {
  loadStyle,
  loadPageStyles,
  preloadPageStyles,
  unloadStyle,
  isStyleLoaded,
  getLoadedStyles
};
