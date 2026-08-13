import { t } from "./i18n.js";

export function formatDateTime(dateStr) {
    if (!dateStr) return '-';
    const date = new Date(dateStr);
    return date.toLocaleString();
}

export function getStatusText(status) {
  const statusMap = {
    'active': t('status.active'),
    'inactive': t('status.inactive'),
    'reserved': t('status.reserved')
  };
  return statusMap[status] || status || t('status.unknown');
}

export function getDeviceTypeName(type) {
  const typeNames = {
    'workstation': t('device.workstation'),
    'cabinet_position': t('device.cabinet_position'),
    'switch': t('device.switch')
  };
  return typeNames[type] || type || '-';
}

export function getRoomTypeName(type) {
  const typeNames = {
    'OFFICE': t('room.type_office'),
    'DATA_CENTER': t('room.type_datacenter'),
    'TELECOM_CLOSET': t('room.type_telecom_closet')
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

  if (diff < 60000) return t('time.just_now');
  if (diff < 3600000) return t('time.minutes_ago', { count: Math.floor(diff / 60000) });
  if (diff < 86400000) return t('time.hours_ago', { count: Math.floor(diff / 3600000) });
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
