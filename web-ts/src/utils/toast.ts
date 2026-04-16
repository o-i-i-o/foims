import type { ToastType } from "../types/common.js";
import { escapeHtml } from "./helpers.js";

let toastContainer: HTMLDivElement | null = null;

function ensureContainer(): HTMLDivElement {
  if (!toastContainer) {
    toastContainer = document.getElementById("toast-container") as HTMLDivElement | null;
    if (!toastContainer) {
      toastContainer = document.createElement("div");
      toastContainer.id = "toast-container";
      toastContainer.className = "toast-container";
      document.body.appendChild(toastContainer);
    }
  }
  return toastContainer;
}

const TOAST_ICONS: Record<ToastType, string> = {
  success: "✓",
  error: "✕",
  warning: "⚠",
  info: "ℹ",
};

export function showToast(message: string, type: ToastType = "info", duration = 3000): HTMLDivElement {
  const container = ensureContainer();

  const toast = document.createElement("div");
  toast.className = `toast toast-${type}`;

  toast.innerHTML = `
        <span class="toast-icon">${TOAST_ICONS[type] || TOAST_ICONS.info}</span>
        <span class="toast-message">${escapeHtml(message)}</span>
        <button class="toast-close" aria-label="Close">×</button>
    `;

  container.appendChild(toast);

  const closeBtn = toast.querySelector(".toast-close") as HTMLButtonElement;
  closeBtn.addEventListener("click", () => removeToast(toast));

  requestAnimationFrame(() => {
    toast.classList.add("toast-visible");
  });

  if (duration > 0) {
    setTimeout(() => removeToast(toast), duration);
  }

  return toast;
}

function removeToast(toast: HTMLDivElement): void {
  toast.classList.remove("toast-visible");
  toast.classList.add("toast-hiding");

  setTimeout(() => {
    if (toast.parentNode) {
      toast.parentNode.removeChild(toast);
    }
  }, 300);
}

export function showSuccess(message: string, duration = 3000): HTMLDivElement {
  return showToast(message, "success", duration);
}

export function showError(message: string, duration = 4000): HTMLDivElement {
  return showToast(message, "error", duration);
}

export function showWarning(message: string, duration = 3500): HTMLDivElement {
  return showToast(message, "warning", duration);
}

export function showInfo(message: string, duration = 3000): HTMLDivElement {
  return showToast(message, "info", duration);
}
