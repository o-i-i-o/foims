import { updatePageTranslations } from './i18n.js';

const loadedModals = new Set();
const loadingModals = new Map();
const htmlCache = new Map();

const MODAL_REGISTRY = {
    'network-type-modal': '/static/modals/network-type-modal.html',
    'network-modal': '/static/modals/network-modal.html',
    'room-modal': '/static/modals/room-modal.html',
    'workstation-modal': '/static/modals/workstation-modal.html',
    'cabinet-modal': '/static/modals/cabinet-modal.html',
    'cabinet-position-modal': '/static/modals/cabinet-position-modal.html',
    'device-ports-group-modal': '/static/modals/device-ports-group-modal.html',
    'device-port-detail-modal': '/static/modals/device-port-detail-modal.html',
    'user-modal': '/static/modals/user-modal.html',
    'two-factor-modal': '/static/modals/two-factor-modal.html',
    'cert-generate-modal': '/static/modals/cert-generate-modal.html',
    'import-result-modal': '/static/modals/import-result-modal.html',
    'cert-import-modal': '/static/modals/cert-import-modal.html',
    'scheduled-task-modal': '/static/modals/scheduled-task-modal.html',
    'task-logs-modal': '/static/modals/task-logs-modal.html',
    'organization-modal': '/static/modals/organization-modal.html',
    'org-template-modal': '/static/modals/org-template-modal.html',
    'org-template-editor-modal': '/static/modals/org-template-editor-modal.html',
    'net-outlet-modal': '/static/modals/net-outlet-modal.html',
    'device-modal': '/static/modals/device-modal.html',
    'room-children-list-modal': '/static/modals/room-children-list-modal.html',
    'cabinet-positions-list-modal': '/static/modals/cabinet-positions-list-modal.html',
};

async function fetchModalHtml(modalId) {
    if (htmlCache.has(modalId)) {
        return htmlCache.get(modalId);
    }

    if (loadingModals.has(modalId)) {
        return loadingModals.get(modalId);
    }

    const url = MODAL_REGISTRY[modalId];
    if (!url) {
        return null;
    }

    const promise = (async () => {
        try {
            const response = await fetch(url);
            if (!response.ok) {
                throw new Error(`Failed to load modal template: ${response.status}`);
            }

            const html = await response.text();

            const match = html.match(/<template[^>]*>([\s\S]*?)<\/template>/);
            const innerHtml = match ? match[1].trim() : html.trim();

            htmlCache.set(modalId, innerHtml);
            return innerHtml;
        } catch (error) {
            console.error(`加载模态框模板失败 [${modalId}]:`, error);
            return null;
        } finally {
            loadingModals.delete(modalId);
        }
    })();

    loadingModals.set(modalId, promise);
    return promise;
}

export async function loadModal(id) {
    if (loadedModals.has(id)) {
        const existing = document.getElementById(id);
        if (existing) {
            return existing;
        }
        loadedModals.delete(id);
    }

    const innerHtml = await fetchModalHtml(id);
    if (!innerHtml) {
        return null;
    }

    const container = document.createElement('div');
    container.innerHTML = innerHtml;
    const modal = container.firstElementChild;

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

export async function openModal(id, title = '') {
    let modal = document.getElementById(id);

    if (!modal) {
        modal = await loadModal(id);
    }

    if (!modal) return;

    modal.classList.add('active');

    if (title) {
        const titleElement = modal.querySelector('.modal-title');
        if (titleElement) {
            titleElement.textContent = title;
        }
    }

    document.body.style.overflow = 'hidden';

    return modal;
}

export function closeModal(id) {
    const modal = document.getElementById(id);

    if (!modal) return;

    modal.classList.remove('active');

    const form = modal.querySelector('form');
    if (form) {
        form.reset();
        const hiddenIdField = form.querySelector('input[type="hidden"]');
        if (hiddenIdField) {
            hiddenIdField.value = '';
        }
    }

    const ipContainers = modal.querySelectorAll('[id$="-ips-container"]');
    ipContainers.forEach(container => {
        container.innerHTML = '';
    });

    document.body.style.overflow = '';
}

export function initModalTemplates() {
}

export function preloadModalsOnIdle() {
}
