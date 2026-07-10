export { showToast } from './toast.js';
export { showConfirm, confirmDelete } from './confirm.js';
export { renderPagination } from './pagination.js';
export { formatDateTime } from './formatter.js';

export const DEFAULT_PAGE_SIZE = 20;

import { renderPagination as renderPaginationFn } from './pagination.js';
import { showToast as showToastFn } from './toast.js';
import { closeModal as closeModalFn } from './modal.js';
import { apiPost, apiPut, apiDelete } from './apiClient.js';
import { showConfirm } from './confirm.js';
import { escapeHtml } from './helpers.js';

export { escapeHtml };

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
        },
        setSort(by, order = null) {
            this.by = by;
            if (order) {
                this.order = order;
            }
            return this;
        },
        get sortBy() {
            return this.by;
        },
        get sortOrder() {
            return this.order;
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
