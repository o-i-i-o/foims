import { t } from "./i18n.js";
import { openModal, closeModal } from "./modalLoader.js";

let confirmResolve = null;

// 模态框 HTML 位于 modals/common/confirm-modal.html，首次调用时懒加载。
// closeModal 会销毁 DOM，下次打开重新注入 —— 此处随注入随绑定，
// 并用 dataset 标志防止"模态框保持打开时再次 showConfirm"造成的重复绑定。
function bindConfirmListeners(modal) {
  if (modal.dataset.confirmBound === "true") {
    return;
  }
  modal.dataset.confirmBound = "true";

  modal.querySelector("#confirm-cancel")?.addEventListener("click", () => {
    hideConfirm(false);
  });

  modal.querySelector("#confirm-ok")?.addEventListener("click", () => {
    hideConfirm(true);
  });

  // 关闭按钮不走全局 data-modal-id 逻辑，必须经由 hideConfirm 结束 Promise
  modal.querySelector("#confirm-close")?.addEventListener("click", () => {
    hideConfirm(false);
  });

  modal.addEventListener("click", (e) => {
    if (e.target === modal) {
      hideConfirm(false);
    }
  });
}

function hideConfirm(result) {
  // 统一走 closeModal（移除 active 并销毁 DOM），与全站模态框生命周期一致；
  // 背景/关闭按钮的监听在元素级触发，先于 document 委托执行，Promise 不会悬挂
  closeModal("confirm-modal");

  if (confirmResolve) {
    confirmResolve(result);
    confirmResolve = null;
  }
}

// Esc 视为取消（仅确认框激活时生效；全站其余模态暂无 Esc 约定）
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !confirmResolve) {
    return;
  }
  const modal = document.getElementById("confirm-modal");
  if (modal?.classList.contains("active")) {
    hideConfirm(false);
  }
});

export function showConfirm(message, options = {}) {
  return new Promise((resolve) => {
    // 并发调用时先结束上一个（视为取消），避免其调用方永久悬挂
    if (confirmResolve) {
      confirmResolve(false);
    }
    confirmResolve = resolve;

    openModal("confirm-modal").then((modal) => {
      // openModal 竞态守卫：openModal 完成时上一个确认框可能已被
      // 新调用替换，晚到的旧回调不得覆写新确认框的文案
      if (confirmResolve !== resolve) {
        return;
      }
      if (!modal) {
        // 模板加载失败时直接返回取消，避免调用方永久挂起
        confirmResolve = null;
        resolve(false);
        return;
      }

      bindConfirmListeners(modal);

      const messageEl = modal.querySelector("#confirm-message");
      if (messageEl) {
        messageEl.textContent = message;
      }

      const titleEl = modal.querySelector(".modal-title");
      if (titleEl && options.title) {
        titleEl.textContent = options.title;
      }

      const okBtn = modal.querySelector("#confirm-ok");
      if (okBtn) {
        okBtn.textContent = options.confirmText || t("common.confirm");
        okBtn.className = `btn ${options.danger ? "btn-danger" : "btn-primary"}`;
      }

      const cancelBtn = modal.querySelector("#confirm-cancel");
      if (cancelBtn) {
        cancelBtn.textContent = options.cancelText || t("common.cancel");
      }
    });
  });
}

export async function confirmDelete(entityName, options = {}) {
  const message = options.message || t("common.confirm_delete", { name: entityName });
  return showConfirm(message, {
    title: t("common.delete"),
    confirmText: t("common.delete"),
    danger: true,
    ...options
  });
}

export default {
  showConfirm,
  confirmDelete
};
