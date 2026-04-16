import { updatePageTranslations } from "./i18n.js";

const modalTemplates = new Map<string, string>();
const loadedModals = new Set<string>();

const MODAL_REGISTRY: Record<string, string> = {
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

export function registerModalTemplate(modalId: string, templateId: string): void {
  modalTemplates.set(modalId, templateId);
}

export function loadModal(id: string): HTMLElement | null {
  if (loadedModals.has(id)) {
    return document.getElementById(id);
  }

  const templateId = modalTemplates.get(id) || MODAL_REGISTRY[id];
  if (!templateId) {
    return null;
  }

  const template = document.getElementById(templateId) as HTMLTemplateElement | null;
  if (!template) {
    return null;
  }

  const clone = template.content.cloneNode(true) as DocumentFragment;
  const modal = clone.firstElementChild as HTMLElement | null;

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

export function openModal(id: string, title = ""): HTMLElement | null {
  let modal = document.getElementById(id);

  if (!modal) {
    modal = loadModal(id);
  }

  if (!modal) return null;

  modal.classList.add("active");

  if (title) {
    const titleElement = modal.querySelector(".modal-title") as HTMLElement | null;
    if (titleElement) {
      titleElement.textContent = title;
    }
  }

  document.body.style.overflow = "hidden";

  return modal;
}

export function closeModal(id: string): void {
  const modal = document.getElementById(id);

  if (!modal) return;

  modal.classList.remove("active");

  const form = modal.querySelector("form") as HTMLFormElement | null;
  if (form) {
    form.reset();
    const hiddenIdField = form.querySelector('input[type="hidden"]') as HTMLInputElement | null;
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

export function unloadModal(id: string): void {
  const modal = document.getElementById(id);
  if (modal) {
    modal.remove();
    loadedModals.delete(id);
  }
}

export function initModalTemplates(): void {
  Object.entries(MODAL_REGISTRY).forEach(([modalId, templateId]) => {
    registerModalTemplate(modalId, templateId);
  });
}

export function isModalLoaded(id: string): boolean {
  return loadedModals.has(id);
}
