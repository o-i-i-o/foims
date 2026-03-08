export { showToast, showSuccess, showError, showWarning, showInfo } from './toast.js';
export { showConfirm, confirmDelete } from './confirm.js';
export { renderPagination, renderPageInfo, createPaginationState } from './pagination.js';

export const DEFAULT_PAGE_SIZE = 20;

import { renderPagination as renderPaginationFn } from './pagination.js';
import { showToast as showToastFn } from './toast.js';
import { closeModal as closeModalFn } from './modal.js';
import { apiPost, apiPut, apiDelete } from './apiClient.js';
import { showConfirm } from './confirm.js';

export function createSortState(defaultBy = 'name', defaultOrder = 'asc') {
    return {
        by: defaultBy,
        order: defaultOrder,
        toggle(key) {
            if (this.by === key) {
                this.order = this.order === 'asc' ? 'desc' : 'asc';
            } else {
                this.by = key;
                this.order = 'asc';
            }
            return this;
        }
    };
}

export function updateSortIcons(tableId, sortState) {
    const table = typeof tableId === 'string' ? document.getElementById(tableId) : tableId;
    if (!table) return;
    
    table.querySelectorAll('th.sortable').forEach(th => {
        const sortKey = th.dataset.sort;
        
        if (sortKey === sortState.by) {
            th.classList.add('sorted', sortState.order);
            th.classList.remove(sortState.order === 'asc' ? 'desc' : 'asc');
        } else {
            th.classList.remove('sorted', 'asc', 'desc');
        }
    });
}

export function initSortEvents(tableId, sortState, loadDataFn) {
    const table = typeof tableId === 'string' ? document.getElementById(tableId) : tableId;
    if (!table) return;
    
    table.querySelectorAll('th.sortable').forEach(th => {
        th.addEventListener('click', () => {
            const sortKey = th.dataset.sort;
            sortState.toggle(sortKey);
            loadDataFn(1, sortState.by, sortState.order);
        });
    });
}

export function createTableState(options = {}) {
    const {
        pageSize = DEFAULT_PAGE_SIZE,
        defaultSortBy = 'name',
        defaultSortOrder = 'asc'
    } = options;
    
    let isLoading = false;
    let currentPage = 1;
    const sortState = createSortState(defaultSortBy, defaultSortOrder);
    
    return {
        get isLoading() { return isLoading; },
        get currentPage() { return currentPage; },
        get sortBy() { return sortState.by; },
        get sortOrder() { return sortState.order; },
        get pageSize() { return pageSize; },
        get sortState() { return sortState; },
        
        setLoading(value) { isLoading = value; },
        setPage(page) { currentPage = page; return this; },
        setSort(by, order) { sortState.by = by; sortState.order = order; return this; },
        
        toggleSort(key) {
            sortState.toggle(key);
            currentPage = 1;
            return this;
        },
        
        getQueryParams() {
            return {
                page: currentPage,
                page_size: pageSize,
                sort_by: sortState.by,
                sort_order: sortState.order
            };
        }
    };
}

export function debounce(func, wait) {
    let timeout;
    return function executedFunction(...args) {
        const later = () => {
            clearTimeout(timeout);
            func(...args);
        };
        clearTimeout(timeout);
        timeout = setTimeout(later, wait);
    };
}

export function throttle(func, limit) {
    let inThrottle;
    return function executedFunction(...args) {
        if (!inThrottle) {
            func(...args);
            inThrottle = true;
            setTimeout(() => inThrottle = false, limit);
        }
    };
}

export function formatDateTime(dateStr) {
    if (!dateStr) return '-';
    const date = new Date(dateStr);
    return date.toLocaleString();
}

export function formatDate(dateStr) {
    if (!dateStr) return '-';
    const date = new Date(dateStr);
    return date.toLocaleDateString();
}

export function formatRelativeTime(dateStr) {
    if (!dateStr) return '-';
    const date = new Date(dateStr);
    const now = new Date();
    const diff = now - date;
    
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    
    if (days > 0) {
        return `${days} 天前`;
    } else if (hours > 0) {
        return `${hours} 小时前`;
    } else if (minutes > 0) {
        return `${minutes} 分钟前`;
    } else {
        return '刚刚';
    }
}

export function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

export function sanitizeHtml(html) {
    if (!html) return '';
    const div = document.createElement('div');
    div.textContent = html;
    return div.innerHTML;
}

export function createElement(tag, attributes = {}, children = []) {
    const element = document.createElement(tag);
    
    Object.entries(attributes).forEach(([key, value]) => {
        if (key === 'className') {
            element.className = value;
        } else if (key.startsWith('data-')) {
            element.dataset[key] = value;
        } else if (key.startsWith('on') && typeof value === 'function') {
            const eventName = key.slice(2).toLowerCase();
            element.addEventListener(eventName, value);
        } else {
            element[key] = value;
        }
    });
    
    if (children.length > 0) {
        const fragment = document.createDocumentFragment();
        children.forEach(child => {
            if (typeof child === 'string') {
                element.insertAdjacentHTML(child);
            } else {
                element.appendChild(child);
            }
        });
    }
    
    return element;
}

export function renderTable(container, dataOrOptions, renderFn, emptyMessage, colSpan) {
    const el = typeof container === 'string' ? document.querySelector(container) : container;
    
    if (!el) return;
    
    let data, columns, empty, colspan;
    
    if (Array.isArray(dataOrOptions)) {
        data = dataOrOptions;
        empty = emptyMessage || '暂无数据';
        colspan = colSpan || 1;
        
        const tbody = el.querySelector('tbody') || el;
        
        if (!data.length) {
            tbody.innerHTML = `<tr><td colspan="${colspan}" class="text-center text-muted">${empty}</td></tr>`;
            return;
        }
        
        tbody.innerHTML = '';
        data.forEach((row, index) => {
            const tr = document.createElement('tr');
            if (renderFn) {
                tr.innerHTML = renderFn(row, index);
            }
            tbody.appendChild(tr);
        });
    } else if (typeof dataOrOptions === 'object' && dataOrOptions !== null) {
        const options = dataOrOptions;
        data = options.data || [];
        columns = options.columns || [];
        empty = options.emptyMessage || '暂无数据';
        const rowIdField = options.rowIdField || 'id';
        const onRowClick = options.onRowClick;
        const onRowDoubleClick = options.onRowDoubleClick;
        
        const tbody = el.querySelector('tbody') || el;
        const thead = el.querySelector('thead');
        const headerColumnCount = thead ? thead.querySelectorAll('th').length : columns.length;
        
        if (!data.length) {
            tbody.innerHTML = `<tr><td colspan="${headerColumnCount || columns.length || 1}" class="text-center text-muted">${empty}</td></tr>`;
            return;
        }
        
        const fragment = document.createDocumentFragment();
        
        data.forEach((row, index) => {
            const tr = document.createElement('tr');
            
            if (rowIdField && row[rowIdField]) {
                tr.dataset.id = row[rowIdField];
            }
            
            columns.forEach(column => {
                const td = document.createElement('td');
                
                if (column.render) {
                    const content = column.render(row[column.field], row, index);
                    if (typeof content === 'string') {
                        td.innerHTML = content;
                    } else if (content instanceof Node) {
                        td.appendChild(content);
                    } else if (content !== undefined && content !== null) {
                        td.textContent = content;
                    }
                } else {
                    td.textContent = row[column.field] ?? '';
                }
                
                if (column.className) {
                    td.className = column.className;
                }
                
                tr.appendChild(td);
            });
            
            if (onRowClick) {
                tr.addEventListener('click', () => onRowClick(row, index));
            }
            
            if (onRowDoubleClick) {
                tr.addEventListener('dblclick', () => onRowDoubleClick(row, index));
            }
            
            fragment.appendChild(tr);
        });
        
        tbody.innerHTML = '';
        tbody.appendChild(fragment);
    }
}

export function showLoading(container, message = '加载中...') {
    if (!container) return;
    
    container.innerHTML = `
        <div class="loading-overlay">
            <div class="loading-spinner"></div>
            <span class="loading-text">${message}</span>
        </div>
    `;
}

export function hideLoading(container) {
    const loading = container?.querySelector('.loading-overlay');
    if (loading) {
        loading.remove();
    }
}

export function setLoading(element, isLoading) {
    if (!element) return;
    
    if (isLoading) {
        element.classList.add('loading');
        element.disabled = true;
    } else {
        element.classList.remove('loading');
        element.disabled = false;
    }
}

export function highlightElement(element, duration = 2000) {
    if (!element) return;
    
    element.classList.add('highlight');
    setTimeout(() => {
        element.classList.remove('highlight');
    }, duration);
}

export function copyToClipboard(text) {
    return navigator.clipboard.writeText(text).then(() => {
        return true;
    }).catch(() => {
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        
        try {
            document.execCommand('copy');
            document.body.removeChild(textarea);
            return true;
        } catch {
            document.body.removeChild(textarea);
            return false;
        }
    });
}

export function getElementValue(id) {
    const element = document.getElementById(id);
    if (!element) return '';
    
    if (element.type === 'checkbox') {
        return element.checked;
    }
    
    return element.value?.trim() ?? '';
}

export function handleError(error, defaultMessage = '操作失败') {
    console.error('Error:', error);
    
    if (error.message) {
        showToastFn(error.message, 'error');
    } else if (typeof error === 'string') {
        showToastFn(error, 'error');
    } else {
        showToastFn(defaultMessage, 'error');
    }
}

export async function handleFormSubmit(config) {
    const { 
        formData, 
        id, 
        baseUrl, 
        successMessage = '保存成功', 
        errorMessage = '保存失败',
        modalId,
        reloadFunction
    } = config;
    
    try {
        let result;
        if (id) {
            result = await apiPut(`${baseUrl}/${id}`, formData);
        } else {
            result = await apiPost(baseUrl, formData);
        }
        
        if (result.success) {
            showToastFn(successMessage, 'success');
            
            if (modalId) {
                closeModalFn(modalId);
            }
            
            if (reloadFunction) {
                await reloadFunction();
            }
            
            return true;
        } else {
            showToastFn(result.message || errorMessage, 'error');
            return false;
        }
    } catch (error) {
        handleError(error, errorMessage);
        return false;
    }
}

export async function handleDelete(id, apiOrCallback, successMessageOrOptions, callbackOrOptions = {}) {
    let apiPath, successMessage, refreshCallback, options;
    
    if (typeof apiOrCallback === 'string') {
        apiPath = apiOrCallback;
        if (typeof successMessageOrOptions === 'string') {
            successMessage = successMessageOrOptions;
            options = typeof callbackOrOptions === 'object' ? callbackOrOptions : {};
        } else {
            successMessage = '删除成功';
            options = successMessageOrOptions || {};
        }
        refreshCallback = typeof callbackOrOptions === 'function' ? callbackOrOptions : null;
    } else {
        options = successMessageOrOptions || {};
        successMessage = options.successMessage || '删除成功';
    }
    
    const confirmMessage = options.confirmMessage || '确定要删除吗？';
    const errorMessage = options.errorMessage || '删除失败';
    
    const confirmed = await showConfirm(confirmMessage);
    if (!confirmed) return { success: false, cancelled: true };
    
    try {
        let result;
        if (apiPath) {
            result = await apiDelete(`${apiPath}/${id}`);
        } else if (typeof apiOrCallback === 'function') {
            result = await apiOrCallback(id);
        } else {
            throw new Error('No delete callback or API path provided');
        }
        
        if (result.success) {
            showToastFn(successMessage, 'success');
            if (refreshCallback) {
                await refreshCallback();
            }
            return { success: true };
        } else {
            showToastFn(result.message || errorMessage, 'error');
            return { success: false, message: result.message };
        }
    } catch (error) {
        handleError(error, errorMessage);
        return { success: false, message: error.message };
    }
}

export function appendPaginationToTable(container, data, onPageChange) {
    const el = typeof container === 'string' ? document.querySelector(container) : container;
    
    if (!el) return;
    
    const tableContainer = el.closest('.table-container');
    let paginationContainer;
    
    if (tableContainer) {
        let existingPagination = tableContainer.querySelector('.pagination-container');
        if (existingPagination) {
            existingPagination.remove();
        }
        paginationContainer = document.createElement('div');
        paginationContainer.className = 'pagination-container';
        tableContainer.appendChild(paginationContainer);
    } else {
        const existingPagination = el.querySelector('.pagination-wrapper');
        if (existingPagination) {
            existingPagination.remove();
        }
        paginationContainer = el;
    }
    
    const total = data.total || 0;
    const currentPage = data.page || 1;
    const pageSize = data.page_size || 20;
    const totalPages = Math.ceil(total / pageSize);
    
    if (totalPages <= 1) {
        if (paginationContainer && paginationContainer.classList.contains('pagination-container')) {
            paginationContainer.innerHTML = '';
        }
        return;
    }
    
    const paginationWrapper = document.createElement('div');
    paginationWrapper.className = 'pagination-wrapper';
    
    renderPaginationFn(paginationWrapper, currentPage, totalPages, onPageChange, total);
    
    paginationContainer.innerHTML = '';
    paginationContainer.appendChild(paginationWrapper);
}

export function removeToast() {
    const toasts = document.querySelectorAll('.toast');
    toasts.forEach(toast => {
        toast.classList.remove('toast-visible');
        setTimeout(() => {
            if (toast.parentNode) {
                toast.parentNode.removeChild(toast);
            }
        }, 300);
    });
}

export default {
    debounce,
    throttle,
    formatDateTime,
    formatDate,
    formatRelativeTime,
    escapeHtml,
    sanitizeHtml,
    createElement,
    renderTable,
    showLoading,
    hideLoading,
    setLoading,
    highlightElement,
    copyToClipboard,
    getElementValue,
    handleError,
    handleFormSubmit,
    handleDelete,
    appendPaginationToTable,
    removeToast
};
