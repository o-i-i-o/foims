import { t } from "./i18n.js";
import { loadModal } from "./modalLoader.js";

let confirmResolve = null;
let confirmModal = null;
let bound = false;

// 模态框 HTML 位于 modals/common/confirm-modal.html，首次调用时懒加载
async function ensureConfirmModal() {
  if (confirmModal && confirmModal.isConnected) {
    return confirmModal;
  }

  confirmModal = await loadModal("confirm-modal");
  if (!confirmModal) {
    return null;
  }

  if (!bound) {
    bound = true;
    confirmModal.querySelector("#confirm-cancel")?.addEventListener("click", () => {
      hideConfirm(false);
    });

    confirmModal.querySelector("#confirm-ok")?.addEventListener("click", () => {
      hideConfirm(true);
    });

    // 关闭按钮不走全局 data-modal-id 逻辑，必须经由 hideConfirm 结束 Promise
    confirmModal.querySelector("#confirm-close")?.addEventListener("click", () => {
      hideConfirm(false);
    });

    confirmModal.addEventListener("click", (e) => {
      if (e.target === confirmModal) {
        hideConfirm(false);
      }
    });
  }

  return confirmModal;
}

function hideConfirm(result) {
  if (confirmModal) {
    confirmModal.classList.remove("active");
  }
  if (confirmResolve) {
    confirmResolve(result);
    confirmResolve = null;
  }
}

export function showConfirm(message, options = {}) {
  return new Promise((resolve) => {
    confirmResolve = resolve;

    ensureConfirmModal().then((modal) => {
      if (!modal) {
        // 模板加载失败时直接返回取消，避免调用方永久挂起
        confirmResolve = null;
        resolve(false);
        return;
      }

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

      modal.classList.add("active");
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
