/**
 * 设备模块共享下拉选项（设备类型/网卡物理类型/接口角色的集中定义）：
 * DeviceModal / DeviceTemplateModal / NetworkCardEditor / PortDetailModal 复用。
 */

/** 下拉选项通用形状：value 为提交值，labelKey 为 i18n 键 */
export interface SelectOption {
  value: string;
  labelKey: string;
}

export const DEVICE_TYPE_OPTIONS: SelectOption[] = [
  { value: "desktop", labelKey: "device_type.desktop" },
  { value: "laptop", labelKey: "device_type.laptop" },
  { value: "printer", labelKey: "device_type.printer" },
  { value: "server", labelKey: "device_type.server" },
  { value: "network_device", labelKey: "device_type.network_device" },
  { value: "switch", labelKey: "device_type.switch" },
  { value: "camera", labelKey: "device_type.camera" },
  { value: "phone", labelKey: "device_type.phone" },
  { value: "other", labelKey: "device_type.other" }
];

/** 网卡形态 */
export const CARD_TYPES: SelectOption[] = [
  { value: "pcie", labelKey: "device.card_type_pcie" },
  { value: "onboard", labelKey: "device.card_type_onboard" },
  { value: "usb", labelKey: "device.card_type_usb" },
  { value: "virtual", labelKey: "device.card_type_virtual" },
  { value: "wwan", labelKey: "device.card_type_wwan" },
  { value: "wifi", labelKey: "device.card_type_wifi" },
  { value: "other", labelKey: "device.card_type_other" }
];

/** 网口物理形态 */
export const PHYSICAL_TYPES: SelectOption[] = [
  { value: "rj45", labelKey: "device.physical_type_rj45" },
  { value: "sfp", labelKey: "device.physical_type_sfp" },
  { value: "sfp_plus", labelKey: "device.physical_type_sfp_plus" },
  { value: "sfp28", labelKey: "device.physical_type_sfp28" },
  { value: "qsfp_plus", labelKey: "device.physical_type_qsfp_plus" },
  { value: "qsfp28", labelKey: "device.physical_type_qsfp28" },
  { value: "wifi", labelKey: "device.physical_type_wifi" },
  { value: "virtual", labelKey: "device.physical_type_virtual" },
  { value: "other", labelKey: "device.physical_type_other" }
];

/** 接口角色 */
export const INTERFACE_ROLES: SelectOption[] = [
  { value: "management", labelKey: "device.interface_role_management" },
  { value: "business", labelKey: "device.interface_role_business" },
  { value: "loopback", labelKey: "device.interface_role_loopback" },
  { value: "uplink", labelKey: "device.interface_role_uplink" },
  { value: "other", labelKey: "device.interface_role_other" }
];

/** i18n 翻译函数最小签名（vue-i18n t 兼容） */
type TranslateFn = (key: string, params?: Record<string, string>) => string;

/** 类型值转展示文案：未注册枚举原样回显，避免键名字面量露出 */
export function getDeviceTypeName(t: TranslateFn, type: string): string {
  const option = DEVICE_TYPE_OPTIONS.find((opt) => opt.value === type);
  return option ? t(option.labelKey) : type;
}
