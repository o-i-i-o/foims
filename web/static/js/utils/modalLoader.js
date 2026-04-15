import { updatePageTranslations } from "./i18n.js";
const modalTemplates = new Map();
const loadedModals = new Set();
const MODAL_REGISTRY = {
    "network-type-modal": "network-type-modal-template",
    "network-modal": "network-modal-template",
    "room-modal": "room-modal-template",
    "workstation-modal": "workstation-modal-template",
    "cabinet-modal": "cabinet-modal-template",
    "cabinet-position-modal": "cabinet-position-modal-template",
    "switch-modal": "switch-modal-template",
    "switch-ports-group-modal": "switch-ports-group-modal-template",
    "switch-port-detail-modal": "switch-port-detail-modal-template",
    "user-modal": "user-modal-template",
    "two-factor-modal": "two-factor-modal-template",
    "cert-generate-modal": "cert-generate-modal-template",
    "import-result-modal": "import-result-modal-template",
    "cert-import-modal": "cert-import-modal-template",
};
export function registerModalTemplate(modalId, templateId) {
    modalTemplates.set(modalId, templateId);
}
export function loadModal(id) {
    if (loadedModals.has(id)) {
        return document.getElementById(id);
    }
    const templateId = modalTemplates.get(id) || MODAL_REGISTRY[id];
    if (!templateId) {
        return null;
    }
    const template = document.getElementById(templateId);
    if (!template) {
        return null;
    }
    const clone = template.content.cloneNode(true);
    const modal = clone.firstElementChild;
    if (!modal) {
        return null;
    }
    const existingModal = document.getElementById(id);
    if (existingModal) {
        existingModal.remove();
    }
    document.body.appendChild(modal);
    loadedModals.add(id);
    updatePageTranslations();
    return modal;
}
export function openModal(id, title = "") {
    let modal = document.getElementById(id);
    if (!modal) {
        modal = loadModal(id);
    }
    if (!modal)
        return null;
    modal.classList.add("active");
    if (title) {
        const titleElement = modal.querySelector(".modal-title");
        if (titleElement) {
            titleElement.textContent = title;
        }
    }
    document.body.style.overflow = "hidden";
    return modal;
}
export function closeModal(id) {
    const modal = document.getElementById(id);
    if (!modal)
        return;
    modal.classList.remove("active");
    const form = modal.querySelector("form");
    if (form) {
        form.reset();
        const hiddenIdField = form.querySelector('input[type="hidden"]');
        if (hiddenIdField) {
            hiddenIdField.value = "";
        }
    }
    const ipContainers = modal.querySelectorAll('[id$="-ips-container"]');
    ipContainers.forEach(container => {
        container.innerHTML = "";
    });
    document.body.style.overflow = "";
}
export function unloadModal(id) {
    const modal = document.getElementById(id);
    if (modal) {
        modal.remove();
        loadedModals.delete(id);
    }
}
export function initModalTemplates() {
    Object.entries(MODAL_REGISTRY).forEach(([modalId, templateId]) => {
        registerModalTemplate(modalId, templateId);
    });
}
export function isModalLoaded(id) {
    return loadedModals.has(id);
}
//# sourceMappingURL=modalLoader.js.map