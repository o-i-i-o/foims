import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";

const tableState = createSortState('name', 'asc');
let currentPage = 1;

const NODE_TYPE_LABELS = {
  campus: t('node.type_campus') || '校区',
  building: t('node.type_building') || '建筑',
  floor: t('node.type_floor') || '楼层',
};

function getNodeTypeName(type) {
  return NODE_TYPE_LABELS[type] || type;
}

function flattenTree(nodes, depth = 0) {
  const result = [];
  for (const node of nodes) {
    result.push({ ...node, depth });
    if (node.children && node.children.length > 0) {
      result.push(...flattenTree(node.children, depth + 1));
    }
  }
  return result;
}

export async function loadNodesData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(`/api/resources/nodes?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const nodes = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#nodes-table", {
      data: nodes,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'node_type', render: (v) => getNodeTypeName(v) },
        { field: 'parent_id', render: (v, row) => row.parent_name || '-' },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#nodes-table", data, loadNodesData);
    }
    updateSortIcons("nodes-table", tableState);
  } catch (error) {
    handleError(error, t('node.load_failed'), () => {
      renderTable("#nodes-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
    });
  }
}

export function initNodeSortEvents() {
  initSortEvents("nodes-table", tableState, loadNodesData);
}

export async function editNode(id) {
  try {
    const result = await apiGet(`/api/resources/nodes/${id}`);
    if (result.success) {
      openNodeModal(result.data);
    } else {
      showToast(`${t('node.load_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('node.load_failed'));
  }
}

export async function deleteNode(id) {
  await handleDelete(id, "/api/resources/nodes", t('node.delete_success'), loadNodesData);
}

async function loadParentNodeOptions(selectedParentId = null) {
  const parentIdSelect = elementCache.get('node-parent-id');
  if (!parentIdSelect) return;

  parentIdSelect.innerHTML = `<option value="">${t('node.no_parent') || '无父节点(根节点)'}</option>`;

  try {
    const result = await apiGet('/api/resources/nodes/tree');
    if (!result.success || !result.data) return;

    const flatNodes = flattenTree(result.data);
    for (const node of flatNodes) {
      const indent = '\u00A0\u00A0\u00A0\u00A0'.repeat(node.depth);
      const typeLabel = getNodeTypeName(node.node_type);
      const option = document.createElement('option');
      option.value = node.id;
      option.textContent = `${indent}${node.name} (${typeLabel})`;
      parentIdSelect.appendChild(option);
    }

    if (selectedParentId) {
      parentIdSelect.value = selectedParentId;
    }
  } catch (error) {
    console.error('加载父节点选项失败:', error);
  }
}

export async function submitNodeForm() {
  const id = getElementValue("node-id");
  const name = getElementValue("node-name");
  const nodeType = getElementValue("node-type");
  const parentId = getElementValue("node-parent-id");
  const description = getElementValue("node-description");

  if (!name?.trim()) {
    showToast(t('node.name_required'), "warning");
    return;
  }

  const nodeData = {
    name: name.trim(),
    node_type: nodeType,
    parent_id: parentId || null,
    description: description?.trim() || null,
  };

  const success = await handleFormSubmit({
    formData: nodeData,
    id,
    baseUrl: "/api/resources/nodes",
    successMessage: t('node.save_success'),
    modalId: "node-modal",
    reloadFunction: loadNodesData
  });

  return success;
}

export async function openNodeModal(node = null) {
  openModal("node-modal");

  const title = elementCache.get('node-modal-title');
  const form = elementCache.get('node-form');

  await loadParentNodeOptions(node?.parent_id);

  if (node) {
    title.textContent = t('node.edit_node');
    elementCache.setValue('node-id', node.id);
    elementCache.setValue('node-name', node.name);
    elementCache.setValue('node-type', node.node_type);
    elementCache.setValue('node-description', node.description || "");

    const parentSelect = elementCache.get('node-parent-id');
    if (parentSelect) parentSelect.disabled = true;
  } else {
    title.textContent = t('node.add_node');
    form.reset();
    elementCache.setValue('node-id', '');

    const parentSelect = elementCache.get('node-parent-id');
    if (parentSelect) parentSelect.disabled = false;
  }
}
