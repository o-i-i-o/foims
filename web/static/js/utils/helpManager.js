/**
 * FOIMS - 卡片/弹窗帮助中心（? 按钮 + 帮助弹窗）
 *
 * 页面不保留常驻帮助文字：字段与流程说明统一收纳于 i18n 的 help 命名空间，
 * 由卡片标题、弹窗标题旁的 ? 图标按钮按需弹出（tooltip 与 title 悬停提示
 * 不占显示区域，仍保留在控件上）。
 *
 * 按钮声明（静态 HTML 与模态外壳通用）：
 *   <button type="button" class="help-btn" data-help="<目标键>"
 *           data-help-title="<标题 i18n key>">?</button>
 * - 图标：模态框内按钮由 modalLoader 的 data-icon 机制注入；
 *   静态页面按钮由 initHelpButtons() 在应用初始化时统一注入
 * - 帮助文案：t(`help.${data-help}`)，按换行分段，"- " 开头的行渲染为列表项
 */
import { t } from "./i18n.js";
import { getIcon } from "./icons.js";
import { openModal } from "./modalLoader.js";

const HELP_ICON = "help";

/** 为 root（默认全文档）内尚未注入图标的帮助按钮填充 SVG（静态卡片按钮用） */
export function hydrateHelpButtons(root = document) {
  root.querySelectorAll("button.help-btn").forEach((btn) => {
    if (!btn.querySelector("svg")) {
      btn.insertAdjacentHTML("afterbegin", getIcon(btn.dataset.icon || HELP_ICON));
    }
  });
}

/**
 * 帮助文案渲染：按行拆分，空行忽略；"- " 开头的连续行合并为一个列表，
 * 其余行各成段落（textContent 写入，文案中的 HTML 元字符按字面显示）
 */
function renderHelpBody(container, text) {
  container.innerHTML = "";
  let list = null;
  text.split("\n").forEach((raw) => {
    const line = raw.trim();
    if (!line) {
      return;
    }
    if (line.startsWith("- ")) {
      if (!list) {
        list = document.createElement("ul");
        container.appendChild(list);
      }
      const li = document.createElement("li");
      li.textContent = line.slice(2);
      list.appendChild(li);
      return;
    }
    list = null;
    const p = document.createElement("p");
    p.textContent = line;
    container.appendChild(p);
  });
}

/** 打开目标帮助弹窗：标题取按钮 data-help-title 的 i18n 文案，正文取 help 命名空间 */
async function openHelp(target, titleKey) {
  const modal = await openModal("help-modal");
  if (!modal) {
    return;
  }
  // 记录来源供语言切换重绘（languagechange 监听按 dataset 重渲）
  modal.dataset.helpTarget = target;
  modal.dataset.helpTitleKey = titleKey || "";
  const title = modal.querySelector(".modal-title");
  if (title) {
    title.textContent = titleKey ? t(titleKey) : t("common.help");
  }
  const body = modal.querySelector("#help-modal-body");
  if (body) {
    renderHelpBody(body, t(`help.${target}`));
  }
}

// 全站事件委托：静态卡片与动态注入模态的帮助按钮统一处理
document.addEventListener("click", (e) => {
  const btn = e.target.closest("button.help-btn[data-help]");
  if (!btn) {
    return;
  }
  e.preventDefault();
  openHelp(btn.dataset.help, btn.dataset.helpTitle || "");
});

// 语言切换时帮助弹窗若处于打开状态，按新语言重绘标题与正文
document.addEventListener("languagechange", () => {
  const modal = document.getElementById("help-modal");
  if (!modal || !modal.classList.contains("active") || !modal.dataset.helpTarget) {
    return;
  }
  const titleKey = modal.dataset.helpTitleKey;
  const title = modal.querySelector(".modal-title");
  if (title) {
    title.textContent = titleKey ? t(titleKey) : t("common.help");
  }
  const body = modal.querySelector("#help-modal-body");
  if (body) {
    renderHelpBody(body, t(`help.${modal.dataset.helpTarget}`));
  }
});

/** 应用初始化时调用：为静态页面上的帮助按钮注入图标 */
export function initHelpButtons() {
  hydrateHelpButtons(document);
}
