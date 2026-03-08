import { apiGet, apiPost, apiPut, apiDelete } from './apiClient.js';
import { showToast } from './toast.js';
import { t } from './i18n.js';

const DEFAULT_PAGE_SIZE = 20;

export function createCrudManager(config) {
    const {
        endpoint,
        entityName,
        entityNameKey,
        loadListCallback,
        renderRowCallback,
        formId,
        modalId,
        validateCallback,
        transformDataCallback,
        afterCreateCallback,
        afterUpdateCallback,
        afterDeleteCallback,
    } = config;

    const nameKey = entityNameKey || entityName;

    return {
        async list(params = {}) {
            const { page = 1, pageSize = DEFAULT_PAGE_SIZE, search = '', ...otherParams } = params;
            const queryParams = new URLSearchParams({
                page: page.toString(),
                page_size: pageSize.toString(),
                ...(search && { search }),
                ...Object.entries(otherParams).reduce((acc, [key, value]) => {
                    if (value !== undefined && value !== null && value !== '') {
                        acc[key] = value.toString();
                    }
                    return acc;
                }, {})
            });

            const result = await apiGet(`${endpoint}?${queryParams}`);
            
            if (result.success) {
                if (loadListCallback) {
                    loadListCallback(result.data);
                }
                return result.data;
            } else {
                showToast(result.message || t('common.load_failed'), 'error');
                return null;
            }
        },

        async get(id) {
            const result = await apiGet(`${endpoint}/${id}`);
            if (!result.success) {
                showToast(result.message || t('common.load_failed'), 'error');
                return null;
            }
            return result.data;
        },

        async create(data, options = {}) {
            const { skipValidation = false, silent = false } = options;
            
            if (validateCallback && !skipValidation) {
                const validationError = validateCallback(data);
                if (validationError) {
                    if (!silent) showToast(validationError, 'error');
                    return { success: false, message: validationError };
                }
            }

            const requestData = transformDataCallback ? transformDataCallback(data) : data;
            const result = await apiPost(endpoint, requestData);

            if (result.success) {
                if (!silent) showToast(t('common.create_success', { name: t(nameKey) }), 'success');
                if (afterCreateCallback) await afterCreateCallback(result.data);
                return { success: true, data: result.data };
            } else {
                if (!silent) showToast(result.message || t('common.create_failed', { name: t(nameKey) }), 'error');
                return { success: false, message: result.message };
            }
        },

        async update(id, data, options = {}) {
            const { skipValidation = false, silent = false } = options;
            
            if (validateCallback && !skipValidation) {
                const validationError = validateCallback(data);
                if (validationError) {
                    if (!silent) showToast(validationError, 'error');
                    return { success: false, message: validationError };
                }
            }

            const requestData = transformDataCallback ? transformDataCallback(data) : data;
            const result = await apiPut(`${endpoint}/${id}`, requestData);

            if (result.success) {
                if (!silent) showToast(t('common.update_success', { name: t(nameKey) }), 'success');
                if (afterUpdateCallback) await afterUpdateCallback(result.data);
                return { success: true, data: result.data };
            } else {
                if (!silent) showToast(result.message || t('common.update_failed', { name: t(nameKey) }), 'error');
                return { success: false, message: result.message };
            }
        },

        async delete(id, options = {}) {
            const { silent = false, confirmMessage = null } = options;

            if (confirmMessage) {
                const confirmed = await showConfirm(confirmMessage);
                if (!confirmed) return { success: false, cancelled: true };
            }

            const result = await apiDelete(`${endpoint}/${id}`);

            if (result.success) {
                if (!silent) showToast(t('common.delete_success', { name: t(nameKey) }), 'success');
                if (afterDeleteCallback) await afterDeleteCallback(id);
                return { success: true };
            } else {
                if (!silent) showToast(result.message || t('common.delete_failed', { name: t(nameKey) }), 'error');
                return { success: false, message: result.message };
            }
        },

        openModal(title = '', data = null) {
            const modal = document.getElementById(modalId);
            if (!modal) return;

            const titleEl = modal.querySelector('.modal-title');
            if (titleEl) titleEl.textContent = title || t(`${nameKey}.add`);

            const form = document.getElementById(formId);
            if (form) {
                form.reset();
                if (data) {
                    this.populateForm(form, data);
                }
            }

            modal.classList.add('active');
            document.body.style.overflow = 'hidden';
        },

        closeModal() {
            const modal = document.getElementById(modalId);
            if (!modal) return;

            modal.classList.remove('active');
            document.body.style.overflow = '';

            const form = document.getElementById(formId);
            if (form) form.reset();
        },

        populateForm(form, data) {
            Object.entries(data).forEach(([key, value]) => {
                const field = form.elements[key] || form.querySelector(`[name="${key}"]`);
                if (field) {
                    if (field.type === 'checkbox') {
                        field.checked = !!value;
                    } else {
                        field.value = value ?? '';
                    }
                }
            });
        },

        getFormData() {
            const form = document.getElementById(formId);
            if (!form) return null;

            const formData = new FormData(form);
            const data = {};
            formData.forEach((value, key) => {
                const field = form.elements[key];
                if (field && field.type === 'checkbox') {
                    data[key] = field.checked;
                } else {
                    data[key] = value;
                }
            });
            return data;
        },

        async submitForm(editId = null) {
            const data = this.getFormData();
            if (!data) return { success: false, message: 'Form not found' };

            if (editId) {
                return await this.update(editId, data);
            } else {
                return await this.create(data);
            }
        }
    };
}

function showConfirm(message) {
    return new Promise((resolve) => {
        const result = window.confirm(message);
        resolve(result);
    });
}

export function createPaginatedLoader(endpoint, options = {}) {
    const { pageSize = DEFAULT_PAGE_SIZE, transform = null } = options;
    let currentPage = 1;
    let totalCount = 0;
    let lastSearch = '';

    return {
        async load(page = 1, search = '', extraParams = {}) {
            currentPage = page;
            lastSearch = search;

            const queryParams = new URLSearchParams({
                page: page.toString(),
                page_size: pageSize.toString(),
                ...(search && { search }),
                ...Object.entries(extraParams).reduce((acc, [key, value]) => {
                    if (value !== undefined && value !== null && value !== '') {
                        acc[key] = value.toString();
                    }
                    return acc;
                }, {})
            });

            const result = await apiGet(`${endpoint}?${queryParams}`);

            if (result.success) {
                const data = result.data;
                totalCount = data.total || 0;
                
                let items = data.items || data || [];
                if (transform) {
                    items = items.map(transform);
                }

                return {
                    items,
                    total: totalCount,
                    page: currentPage,
                    pageSize,
                    totalPages: Math.ceil(totalCount / pageSize)
                };
            }

            return { items: [], total: 0, page: 1, pageSize, totalPages: 0 };
        },

        async next(extraParams = {}) {
            const totalPages = Math.ceil(totalCount / pageSize);
            if (currentPage < totalPages) {
                return await this.load(currentPage + 1, lastSearch, extraParams);
            }
            return null;
        },

        async prev(extraParams = {}) {
            if (currentPage > 1) {
                return await this.load(currentPage - 1, lastSearch, extraParams);
            }
            return null;
        },

        getPage() { return currentPage; },
        getTotal() { return totalCount; },
        getTotalPages() { return Math.ceil(totalCount / pageSize); }
    };
}

export default {
    createCrudManager,
    createPaginatedLoader
};
