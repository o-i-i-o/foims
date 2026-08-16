/**
 * IP 地址与 CIDR 工具函数（IPv4/IPv6）。
 *
 * 由 ipconfig.js 与 networkCardManager.js 此前各自维护的重复实现收敛而来，
 * 两处行为差异已统一：IPv4 八位组拒绝前导零（如 "01"，避免歧义）。
 */

/** 判断是否为 IPv6 地址（按是否含冒号粗判，配合 isValidIPv6 使用）。 */
export function isIPv6(ip) {
  return ip.includes(":");
}

/** 校验 IPv4 点分十进制格式（拒绝前导零）。 */
export function isValidIPv4(ip) {
  const parts = ip.split(".");
  if (parts.length !== 4) return false;
  for (const part of parts) {
    if (!/^\d+$/.test(part)) return false;
    const num = parseInt(part, 10);
    if (isNaN(num) || num < 0 || num > 255) return false;
    if (part.length > 1 && part.startsWith("0") && num !== 0) return false;
  }
  return true;
}

/** 校验 IPv6 格式（支持 "::" 缩写与 zone 后缀，不支持内嵌 IPv4）。 */
export function isValidIPv6(ip) {
  if (!ip.includes(":")) return false;

  const addr = ip.split("%")[0];
  if (addr === "::") return true;

  const parts = addr.split(":");
  if (parts.length > 8) return false;

  const doubleColonCount = parts.filter((p) => p === "").length;
  if (doubleColonCount > 1) return false;
  if (doubleColonCount === 1 && parts.length >= 8) return false;

  for (const part of parts) {
    if (part === "") continue;
    if (!/^[0-9a-fA-F]{1,4}$/.test(part)) return false;
  }

  return true;
}

/** 校验任意 IP 地址（IPv4 或 IPv6）。 */
export function isValidIP(ip) {
  if (!ip || typeof ip !== "string") return false;
  return isValidIPv4(ip) || isValidIPv6(ip);
}

/** IPv4 点分十进制 → BigInt，非法输入返回 null。 */
export function ipv4ToInt(ip) {
  const parts = ip.split(".").map((p) => parseInt(p, 10));
  if (parts.length !== 4 || parts.some((p) => isNaN(p) || p < 0 || p > 255)) {
    return null;
  }
  return (
    (BigInt(parts[0]) << 24n) +
    (BigInt(parts[1]) << 16n) +
    (BigInt(parts[2]) << 8n) +
    BigInt(parts[3])
  );
}

/** IPv6 → 128 位 BigInt（展开 "::" 缩写），非法输入返回 null。 */
export function ipv6ToInt(ipv6) {
  let ip = ipv6.split("%")[0];
  if (ip === "::") ip = "::0";

  const parts = ip.split(":");
  if (parts.length > 8) return null;

  const doubleColonIndex = parts.indexOf("");
  if (doubleColonIndex !== -1) {
    const missingCount = 8 - parts.length + 1;
    parts.splice(doubleColonIndex, 1, ...Array(missingCount).fill("0"));
  }

  if (parts.length !== 8) return null;

  let result = BigInt(0);
  for (const part of parts) {
    const val = part === "" ? 0 : parseInt(part, 16);
    if (isNaN(val) || val < 0 || val > 65535) return null;
    result = result * BigInt(65536) + BigInt(val);
  }

  return result;
}

/** IPv4 CIDR（如 "192.168.1.0/24"）→ {start, end} 数值范围，非法返回 null。 */
export function ipv4CidrToRange(cidr) {
  const [ip, prefixLen] = cidr.split("/");
  const prefix = parseInt(prefixLen, 10);
  if (!ip || isNaN(prefix) || prefix < 0 || prefix > 32) return null;

  const ipInt = ipv4ToInt(ip);
  if (ipInt === null) return null;

  const max = (1n << 32n) - 1n;
  const mask = prefix === 0 ? 0n : max ^ ((1n << BigInt(32 - prefix)) - 1n);
  const network = ipInt & mask;
  const broadcast = network | (max ^ mask);

  return { start: network, end: broadcast };
}

/** IPv6 CIDR → {start, end} 数值范围，非法返回 null。 */
export function ipv6CidrToRange(cidr) {
  const [ip, prefixLen] = cidr.split("/");
  const prefix = parseInt(prefixLen, 10);
  if (!ip || isNaN(prefix) || prefix < 0 || prefix > 128) return null;

  const ipInt = ipv6ToInt(ip);
  if (ipInt === null) return null;

  const max = (1n << 128n) - 1n;
  const mask = prefix === 0 ? 0n : max ^ ((1n << BigInt(128 - prefix)) - 1n);
  const network = ipInt & mask;
  const broadcast = network | (max ^ mask);

  return { start: network, end: broadcast };
}

/** 判断 IP 是否落在 CIDR 网段内（按地址族自动选择 v4/v6）。 */
export function isIpInCidr(ipAddress, cidr) {
  if (!cidr) return true;

  if (isIPv6(ipAddress)) {
    const ipInt = ipv6ToInt(ipAddress);
    if (ipInt === null) return false;

    const range = ipv6CidrToRange(cidr);
    if (!range) return false;

    return ipInt >= range.start && ipInt <= range.end;
  }

  const ipInt = ipv4ToInt(ipAddress);
  if (ipInt === null) return false;

  const range = ipv4CidrToRange(cidr);
  if (!range) return false;

  return ipInt >= range.start && ipInt <= range.end;
}
