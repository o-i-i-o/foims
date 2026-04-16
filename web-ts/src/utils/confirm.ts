import type { ConfirmOptions } from "../types/common.js";
import { t } from "./i18n.js";

let confirmResolve: ((result: boolean) => void) | null = null;
let confirmModal: HTMLDivElement | null = null;

function createConfirmModal(): HTMLDivElement {
  if (confirmModal) return confirmModal;

  confirmModal = document.createElement("div");
  confirmModal.id = "confirm-modal";
  confirmModal.className = "modal";
  confirmModal.innerHTML = `
        <div class="modal-content modal-sm">
            <div class="modal-header">
                <h3 class="modal-title">${t("common.confirm")}</h3>
                <button class="modal-close" data-modal-id="confirm-modal">&times;</button>
            </div>
            <div class="modal-body">
                <p id="confirm-message"></p>
            </div>
            <div class="modal-footer">
                <button type="button" class="btn btn-secondary" id="confirm-cancel">${t("common.cancel")}</button>
                <button type="button" class="btn btn-danger" id="confirm-ok">${t("common.confirm")}</button>
            </div>
        </div>
    `;

  document.body.appendChild(confirmModal);

  confirmModal.querySelector("#confirm-cancel")!.addEventListener("click", () => {
    hideConfirm(false);
  });

  confirmModal.querySelector("#confirm-ok")!.addEventListener("click", () => {
    hideConfirm(true);
  });

  confirmModal.querySelector(".modal-close")!.addEventListener("click", () => {
    hideConfirm(false);
  });

  confirmModal.addEventListener("click", (e: MouseEvent) => {
    if (e.target === confirmModal) {
      hideConfirm(false);
    }
  });

  return confirmModal;
}

function hideConfirm(result: boolean): void {
  if (confirmModal) {
    confirmModal.classList.remove("active");
  }
  if (confirmResolve) {
    confirmResolve(result);
    confirmResolve = null;
  }
}

export function showConfirm(message: string, options: ConfirmOptions = {}): Promise<boolean> {
  return new Promise((resolve) => {
    confirmResolve = resolve;

    const modal = createConfirmModal();
    const messageEl = modal.querySelector("#confirm-message") as HTMLParagraphElement | null;

    if (messageEl) {
      messageEl.textContent = message;
    }

    const titleEl = modal.querySelector(".modal-title") as HTMLHeadingElement | null;
    if (titleEl && options.title) {
      titleEl.textContent = options.title;
    }

    const okBtn = modal.querySelector("#confirm-ok") as HTMLButtonElement | null;
    if (okBtn) {
      okBtn.textContent = options.confirmText || t("common.confirm");
      okBtn.className = `btn ${options.danger ? "btn-danger" : "btn-primary"}`;
    }

    const cancelBtn = modal.querySelector("#confirm-cancel") as HTMLButtonElement | null;
    if (cancelBtn) {
      cancelBtn.textContent = options.cancelText || t("common.cancel");
    }

    modal.classList.add("active");
  });
}

export async function confirmDelete(entityName: string, options: ConfirmOptions = {}): Promise<boolean> {
  const message = options.message || t("common.confirm_delete", { name: entityName });
  return showConfirm(message, {
    title: t("common.delete"),
    confirmText: t("common.delete"),
    danger: true,
    ...options,
  });
}
