/**
 * IP 地址与 CIDR 工具函数（IPv4/IPv6）。
 *
 * IPv4 八位组拒绝前导零（如 "01"，避免八进制歧义）。
 * 本文件收录前端已接线页面的依赖链（网段弹窗校验、设备网卡 IP 校验）：
 * isIPv6 / isValidIP / isValidIPv4 / isValidIPv6 / isIpInCidr 及其内部实现，
 * 未收录的函数按页面所需再补。
 */

/** 判断是否为 IPv6 地址（按是否含冒号粗判，配合 isValidIPv6 使用）。 */
export function isIPv6(ip: string): boolean {
  return ip.includes(":");
}

/** 校验 IP 地址（IPv4 或 IPv6 任一合法即通过，网卡 IP 行校验用）。 */
export function isValidIP(ip: string): boolean {
  return isValidIPv4(ip) || isValidIPv6(ip);
}

/** 校验 IPv4 点分十进制格式（拒绝前导零）。 */
export function isValidIPv4(ip: string): boolean {
  const parts = ip.split(".");
  if (parts.length !== 4) {
    return false;
  }
  for (const part of parts) {
    if (!/^\d+$/.test(part)) {
      return false;
    }
    const num = Number.parseInt(part, 10);
    if (Number.isNaN(num) || num < 0 || num > 255) {
      return false;
    }
    // 前导零一律拒绝（"01" 与 "00" 同口径）：
    // 点分十进制中前导零有八进制歧义（部分解析器把 010 当 8）
    if (part.length > 1 && part.startsWith("0")) {
      return false;
    }
  }
  return true;
}

/**
 * 将 IPv6 地址（支持 "::" 缩写与 zone 后缀）展开为 8 组十六进制串，
 * 非法输入返回 null。isValidIPv6 与 ipv6ToInt 共用，保证判定口径一致。
 *
 * "::" 在首/尾时 split 会产生两个空段，历史上按"空段计数"实现会把
 * "::1"、"1::" 误判为出现两次缩写而拒绝 —— 正确做法是按 "::" 切片展开。
 */
function expandIPv6Groups(ipv6: string): string[] | null {
  // split 结果首元素必然存在，?? "" 仅为满足索引访问类型
  const chunks = (ipv6.split("%")[0] ?? "").split("::");
  if (chunks.length > 2) {
    return null;
  }

  let groups: string[];
  if (chunks.length === 1) {
    groups = (chunks[0] ?? "").split(":");
  } else {
    const leftPart = chunks[0] ?? "";
    const rightPart = chunks[1] ?? "";
    const left = leftPart === "" ? [] : leftPart.split(":");
    const right = rightPart === "" ? [] : rightPart.split(":");
    // "::" 两侧不得再夹空组（拒绝 "fe80:::1"）；缩写至少代表一个零组，
    // 故两侧显式组合计不得超过 7
    if (left.includes("") || right.includes("") || left.length + right.length > 7) {
      return null;
    }
    groups = [...left, ...Array<string>(8 - left.length - right.length).fill("0"), ...right];
  }

  if (groups.length !== 8) {
    return null;
  }
  return groups.every((g) => /^[0-9a-fA-F]{1,4}$/.test(g)) ? groups : null;
}

/** 校验 IPv6 格式（支持 "::" 缩写与 zone 后缀，不支持内嵌 IPv4）。 */
export function isValidIPv6(ip: string): boolean {
  if (!ip.includes(":")) {
    return false;
  }
  return expandIPv6Groups(ip) !== null;
}

/** IPv4 点分十进制 → BigInt，非法输入返回 null。 */
function ipv4ToInt(ip: string): bigint | null {
  const parts = ip.split(".").map((p) => Number.parseInt(p, 10));
  if (parts.length !== 4 || parts.some((p) => Number.isNaN(p) || p < 0 || p > 255)) {
    return null;
  }
  // 长度与 0-255 校验已通过，四个八位组必然存在，按元组收窄
  const octets = parts as [number, number, number, number];
  return (
    (BigInt(octets[0]) << 24n) +
    (BigInt(octets[1]) << 16n) +
    (BigInt(octets[2]) << 8n) +
    BigInt(octets[3])
  );
}

/** IPv6 → 128 位 BigInt（展开 "::" 缩写），非法输入返回 null。 */
function ipv6ToInt(ipv6: string): bigint | null {
  const groups = expandIPv6Groups(ipv6);
  if (!groups) {
    return null;
  }

  let result = BigInt(0);
  for (const group of groups) {
    result = result * BigInt(65536) + BigInt(Number.parseInt(group, 16));
  }

  return result;
}

/** IPv4 CIDR（如 "192.168.1.0/24"）→ {start, end} 数值范围，非法返回 null。 */
function ipv4CidrToRange(cidr: string): { start: bigint; end: bigint } | null {
  const [ip, prefixLen] = cidr.split("/");
  // 缺省前缀长度按 NaN 处理（parseInt(undefined) 等价 parseInt("")）
  const prefix = Number.parseInt(prefixLen ?? "", 10);
  if (!ip || Number.isNaN(prefix) || prefix < 0 || prefix > 32) {
    return null;
  }

  const ipInt = ipv4ToInt(ip);
  if (ipInt === null) {
    return null;
  }

  const max = (1n << 32n) - 1n;
  const mask = prefix === 0 ? 0n : max ^ ((1n << BigInt(32 - prefix)) - 1n);
  const network = ipInt & mask;
  const broadcast = network | (max ^ mask);

  return { start: network, end: broadcast };
}

/** IPv6 CIDR → {start, end} 数值范围，非法返回 null。 */
function ipv6CidrToRange(cidr: string): { start: bigint; end: bigint } | null {
  const [ip, prefixLen] = cidr.split("/");
  // 缺省前缀长度按 NaN 处理（parseInt(undefined) 等价 parseInt("")）
  const prefix = Number.parseInt(prefixLen ?? "", 10);
  if (!ip || Number.isNaN(prefix) || prefix < 0 || prefix > 128) {
    return null;
  }

  const ipInt = ipv6ToInt(ip);
  if (ipInt === null) {
    return null;
  }

  const max = (1n << 128n) - 1n;
  const mask = prefix === 0 ? 0n : max ^ ((1n << BigInt(128 - prefix)) - 1n);
  const network = ipInt & mask;
  const broadcast = network | (max ^ mask);

  return { start: network, end: broadcast };
}

/**
 * IP 去重键：IPv6 按 128 位数值归一（"2001:0db8::1" 与 "2001:db8::1" 同键），
 * IPv4 按修剪后的文本归一。仅用于同一表单内的地址去重，不承担格式校验：
 * 非法输入原样参与比较，最终由校验链拒绝。
 */
export function ipKey(ip: string): string {
  const text = ip.trim();
  if (isIPv6(text)) {
    const value = ipv6ToInt(text);
    if (value !== null) {
      return `v6:${value.toString(16)}`;
    }
  }
  return `v4:${text}`;
}

/** 判断 IP 是否落在 CIDR 网段内（按地址族自动选择 v4/v6）。 */
export function isIpInCidr(ipAddress: string, cidr: string | null | undefined): boolean {
  if (!cidr) {
    return true;
  }

  if (isIPv6(ipAddress)) {
    const ipInt = ipv6ToInt(ipAddress);
    if (ipInt === null) {
      return false;
    }

    const range = ipv6CidrToRange(cidr);
    if (!range) {
      return false;
    }

    return ipInt >= range.start && ipInt <= range.end;
  }

  const ipInt = ipv4ToInt(ipAddress);
  if (ipInt === null) {
    return false;
  }

  const range = ipv4CidrToRange(cidr);
  if (!range) {
    return false;
  }

  return ipInt >= range.start && ipInt <= range.end;
}
