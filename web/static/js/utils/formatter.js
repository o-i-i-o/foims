import { t } from "./i18n.js";

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

export function getStatusText(status) {
  const statusMap = {
    'active': t('status.active', '活跃'),
    'inactive': t('status.inactive', '不活跃'),
    'reserved': t('status.reserved', '保留')
  };
  return statusMap[status] || status || t('status.unknown', '未知');
}

export function getDeviceTypeName(type) {
  const typeNames = {
    'workstation': t('device.workstation', '工位'),
    'cabinet_position': t('device.cabinet_position', '机位'),
    'switch': t('device.switch', '交换机')
  };
  return typeNames[type] || type || '-';
}

export function getRoomTypeName(type) {
  const typeNames = {
    'OFFICE': t('room.office', '办公室'),
    'DATA_CENTER': t('room.data_center', '数据中心')
  };
  return typeNames[type] || type || '-';
}

export function getActionIcon(action) {
  if (!action) return '📋';
  const actionLower = action.toLowerCase();
  if (actionLower.includes('create') || actionLower.includes('add')) return '➕';
  if (actionLower.includes('update') || actionLower.includes('edit')) return '✏️';
  if (actionLower.includes('delete') || actionLower.includes('remove')) return '🗑️';
  if (actionLower.includes('login')) return '🔐';
  if (actionLower.includes('logout')) return '🚪';
  return '📋';
}

export function formatTime(timestamp) {
  if (!timestamp) return '-';
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now - date;

  if (diff < 60000) return t('time.just_now', '刚刚');
  if (diff < 3600000) return t('time.minutes_ago', { count: Math.floor(diff / 60000) });
  if (diff < 86400000) return t('time.hours_ago', { count: Math.floor(diff / 3600000) });
  return date.toLocaleDateString();
}

export function formatRelativeTime(timestamp) {
  if (!timestamp) return '-';
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now - date;

  if (diff < 60000) return t('time.just_now', '刚刚');
  if (diff < 3600000) return t('time.minutes_ago', { count: Math.floor(diff / 60000) });
  if (diff < 86400000) return t('time.hours_ago', { count: Math.floor(diff / 3600000) });
  if (diff < 604800000) return t('time.days_ago', { count: Math.floor(diff / 86400000) });
  return date.toLocaleDateString();
}

export function getOperationTypeText(action) {
  if (!action) return '-';
  return t(`logs.operation_types.${action}`, action);
}

export function getResourceTypeText(resourceType) {
  if (!resourceType) return '-';
  return t(`logs.resource_types.${resourceType}`, resourceType);
}
