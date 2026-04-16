import { t } from './i18n.js';

let confirmResolve = null;
let confirmModal = null;

function createConfirmModal() {
    if (confirmModal) return confirmModal;
    
    confirmModal = document.createElement('div');
    confirmModal.id = 'confirm-modal';
    confirmModal.className = 'modal';
    confirmModal.innerHTML = `
        <div class="modal-content modal-sm">
            <div class="modal-header">
                <h3 class="modal-title">${t('common.confirm')}</h3>
                <button class="modal-close" data-modal-id="confirm-modal">&times;</button>
            </div>
            <div class="modal-body">
                <p id="confirm-message"></p>
            </div>
            <div class="modal-footer">
                <button type="button" class="btn btn-secondary" id="confirm-cancel">${t('common.cancel')}</button>
                <button type="button" class="btn btn-danger" id="confirm-ok">${t('common.confirm')}</button>
            </div>
        </div>
    `;
    
    document.body.appendChild(confirmModal);
    
    confirmModal.querySelector('#confirm-cancel').addEventListener('click', () => {
        hideConfirm(false);
    });
    
    confirmModal.querySelector('#confirm-ok').addEventListener('click', () => {
        hideConfirm(true);
    });
    
    confirmModal.querySelector('.modal-close').addEventListener('click', () => {
        hideConfirm(false);
    });
    
    confirmModal.addEventListener('click', (e) => {
        if (e.target === confirmModal) {
            hideConfirm(false);
        }
    });
    
    return confirmModal;
}

function hideConfirm(result) {
    if (confirmModal) {
        confirmModal.classList.remove('active');
    }
    if (confirmResolve) {
        confirmResolve(result);
        confirmResolve = null;
    }
}

export function showConfirm(message, options = {}) {
    return new Promise((resolve) => {
        confirmResolve = resolve;
        
        const modal = createConfirmModal();
        const messageEl = modal.querySelector('#confirm-message');
        
        if (messageEl) {
            messageEl.textContent = message;
        }
        
        const titleEl = modal.querySelector('.modal-title');
        if (titleEl && options.title) {
            titleEl.textContent = options.title;
        }
        
        const okBtn = modal.querySelector('#confirm-ok');
        if (okBtn) {
            okBtn.textContent = options.confirmText || t('common.confirm');
            okBtn.className = `btn ${options.danger ? 'btn-danger' : 'btn-primary'}`;
        }
        
        const cancelBtn = modal.querySelector('#confirm-cancel');
        if (cancelBtn) {
            cancelBtn.textContent = options.cancelText || t('common.cancel');
        }
        
        modal.classList.add('active');
    });
}

export async function confirmDelete(entityName, options = {}) {
    const message = options.message || t('common.confirm_delete', { name: entityName });
    return showConfirm(message, {
        title: t('common.delete'),
        confirmText: t('common.delete'),
        danger: true,
        ...options
    });
}

export default {
    showConfirm,
    confirmDelete
};
