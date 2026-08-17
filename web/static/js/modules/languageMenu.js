/**
 * 语言菜单模块
 *
 * 侧边栏语言按钮的下拉选择菜单。语言列表由 i18n 已加载的翻译文件动态提供
 * （每个语言文件的 language.native_name 即展示名），新增语言无需改动本模块。
 * 展开状态在按钮上方弹出、右对齐；收起状态从图标栏右侧飞出。
 */

import { changeLanguage, getCurrentLanguage, getSupportedLanguages, t } from "../utils/i18n.js";
import { getIcon } from "../utils/icons.js";

/**
 * 初始化语言菜单（仅 main 页的按钮控件生效，登录页等 select 控件不经过此处）
 */
export function initLanguageMenu() {
  const btn = document.getElementById("language-selector");
  if (!btn || btn.tagName !== "BUTTON") {
    return;
  }

  let menu = null;

  const closeMenu = () => {
    if (!menu) {
      return;
    }
    menu.remove();
    menu = null;
    btn.setAttribute("aria-expanded", "false");
  };

  const positionMenu = () => {
    if (!menu) {
      return;
    }

    const rect = btn.getBoundingClientRect();
    const menuWidth = menu.offsetWidth;
    const menuHeight = menu.offsetHeight;

    // 收起态从窄栏右侧飞出；展开态右对齐按钮
    let left = document.body.classList.contains("sidebar-collapsed")
      ? rect.right + 8
      : rect.right - menuWidth;
    left = Math.max(8, Math.min(left, window.innerWidth - menuWidth - 8));

    // 页脚位于底部，默认向上弹出，空间不足时向下
    let top = rect.top - menuHeight - 8;
    if (top < 8) {
      top = rect.bottom + 8;
    }

    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
  };

  const buildMenu = () => {
    const current = getCurrentLanguage();
    const el = document.createElement("div");
    el.className = "lang-menu";
    el.setAttribute("role", "menu");
    el.setAttribute("aria-label", t("language.title"));

    for (const { code, nativeName } of getSupportedLanguages()) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "lang-menu-item";
      item.setAttribute("role", "menuitemradio");
      item.dataset.lang = code;
      item.setAttribute("aria-checked", String(code === current));
      // 对勾占位保证各项文字对齐，未选中项由 CSS 隐藏
      item.insertAdjacentHTML("afterbegin", getIcon("check"));
      item.appendChild(document.createTextNode(nativeName));
      item.addEventListener("click", () => {
        if (code !== getCurrentLanguage()) {
          changeLanguage(code);
        }
        closeMenu();
      });
      el.appendChild(item);
    }

    return el;
  };

  btn.addEventListener("click", (e) => {
    // 阻止冒泡，避免触发 document 级关闭逻辑导致菜单刚打开就被关闭
    e.stopPropagation();
    if (menu) {
      closeMenu();
      return;
    }
    menu = buildMenu();
    document.body.appendChild(menu);
    positionMenu();
    btn.setAttribute("aria-expanded", "true");
  });

  document.addEventListener("click", (e) => {
    if (menu && !menu.contains(e.target) && e.target !== btn) {
      closeMenu();
    }
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closeMenu();
    }
  });

  // 视口变化后定位失效，直接关闭
  window.addEventListener("resize", closeMenu);
  window.addEventListener("scroll", closeMenu, true);
}
