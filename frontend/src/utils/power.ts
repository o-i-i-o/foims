/**
 * 功率输入换算（移植自旧版 utils/helpers.js readPowerWattsInput，
 * 房间/机柜弹窗的功率输入与单位下拉共用）。
 */

/**
 * 读取功率输入并统一换算为瓦（整数）：
 * - 空串返回 null（语义：不设置/不记录）；
 * - 千瓦乘 1000（容差 1e-9 吸收浮点误差，0.1kW -> 100W）；
 * - 负数、非数字、换算后非整数瓦或超过 10000000W 返回 NaN（交由调用方提示）。
 */
export function readPowerWattsInput(value: string | number | null, unit: string): number | null {
  if (value === null || String(value).trim() === "") {
    return null;
  }
  const num = Number(value);
  if (!Number.isFinite(num) || num < 0) {
    return NaN;
  }
  const watts = unit === "kW" ? num * 1000 : num;
  if (Math.abs(watts - Math.round(watts)) > 1e-9 || Math.round(watts) > 10000000) {
    return NaN;
  }
  return Math.round(watts);
}
