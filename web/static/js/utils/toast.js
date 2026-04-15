import { escapeHtml } from "./helpers.js";
let toastContainer = null;
function ensureContainer() {
    if (!toastContainer) {
        toastContainer = document.getElementById("toast-container");
        if (!toastContainer) {
            toastContainer = document.createElement("div");
            toastContainer.id = "toast-container";
            toastContainer.className = "toast-container";
            document.body.appendChild(toastContainer);
        }
    }
    return toastContainer;
}
const TOAST_ICONS = {
    success: "✓",
    error: "✕",
    warning: "⚠",
    info: "ℹ",
};
export function showToast(message, type = "info", duration = 3000) {
    const container = ensureContainer();
    const toast = document.createElement("div");
    toast.className = `toast toast-${type}`;
    toast.innerHTML = `
        <span class="toast-icon">${TOAST_ICONS[type] || TOAST_ICONS.info}</span>
        <span class="toast-message">${escapeHtml(message)}</span>
        <button class="toast-close" aria-label="Close">×</button>
    `;
    container.appendChild(toast);
    const closeBtn = toast.querySelector(".toast-close");
    closeBtn.addEventListener("click", () => removeToast(toast));
    requestAnimationFrame(() => {
        toast.classList.add("toast-visible");
    });
    if (duration > 0) {
        setTimeout(() => removeToast(toast), duration);
    }
    return toast;
}
function removeToast(toast) {
    toast.classList.remove("toast-visible");
    toast.classList.add("toast-hiding");
    setTimeout(() => {
        if (toast.parentNode) {
            toast.parentNode.removeChild(toast);
        }
    }, 300);
}
export function showSuccess(message, duration = 3000) {
    return showToast(message, "success", duration);
}
export function showError(message, duration = 4000) {
    return showToast(message, "error", duration);
}
export function showWarning(message, duration = 3500) {
    return showToast(message, "warning", duration);
}
export function showInfo(message, duration = 3000) {
    return showToast(message, "info", duration);
}
//# sourceMappingURL=toast.js.map