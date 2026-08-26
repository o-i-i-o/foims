// IP/CIDR 工具函数单元测试（utils/network.js，前端 IP 校验唯一权威实现）
import {
  isIPv6,
  isValidIPv4,
  isValidIPv6,
  isValidIP,
  ipv4ToInt,
  ipv6ToInt,
  ipv4CidrToRange,
  ipv6CidrToRange,
  isIpInCidr
} from "../static/js/utils/network.js";

describe("isValidIPv4", () => {
  it.each(["0.0.0.0", "192.168.1.1", "10.0.0.1", "255.255.255.255"])("接受合法地址 %s", (ip) => {
    expect(isValidIPv4(ip)).toBe(true);
  });

  it.each([
    "01.2.3.4", // 前导零（与后端约定一致拒绝）
    "256.1.1.1", // 八位组越界
    "1.2.3", // 段数不足
    "1.2.3.4.5", // 段数超限
    "a.b.c.d",
    "1.2.3.-4",
    ""
  ])("拒绝非法地址 %s", (ip) => {
    expect(isValidIPv4(ip)).toBe(false);
  });
});

describe("isValidIPv6", () => {
  it.each([
    "::",
    "::1",
    "1::",
    "fe80::1",
    "fe80::1%eth0", // zone 后缀
    "2001:db8::8a2e:370:7334",
    "2001:0db8:0000:0000:0000:0000:0000:0001",
    "::1:2:3:4:5:6:7", // "::" + 7 个显式组（缩写仅代表一个零组，RFC 合法）
    "1:2:3:4:5:6:7::"
  ])("接受合法地址 %s", (ip) => {
    expect(isValidIPv6(ip)).toBe(true);
  });

  it.each([
    "192.168.1.1", // 无冒号
    "1:2:3:4:5:6:7:8:9", // 超过 8 组
    "fe80:::1", // 三连冒号
    "2001:db8::1::2", // "::" 出现两次
    "gggg::1", // 非十六进制
    "::1:2:3:4:5:6:7:8", // "::" + 8 个显式组，超出 128 位
    ":"
  ])("拒绝非法地址 %s", (ip) => {
    expect(isValidIPv6(ip)).toBe(false);
  });
});

describe("isValidIP / isIPv6", () => {
  it("按地址族各自放行", () => {
    expect(isValidIP("192.168.1.1")).toBe(true);
    expect(isValidIP("fe80::1")).toBe(true);
    expect(isValidIP(null)).toBe(false);
    expect(isValidIP(123)).toBe(false);
  });

  it("isIPv6 按冒号粗判", () => {
    expect(isIPv6("::1")).toBe(true);
    expect(isIPv6("10.0.0.1")).toBe(false);
  });
});

describe("ipv4ToInt", () => {
  it("数值转换正确", () => {
    expect(ipv4ToInt("0.0.0.0")).toBe(0n);
    expect(ipv4ToInt("0.0.0.1")).toBe(1n);
    expect(ipv4ToInt("192.168.1.1")).toBe((192n << 24n) + (168n << 16n) + (1n << 8n) + 1n);
    expect(ipv4ToInt("255.255.255.255")).toBe((1n << 32n) - 1n);
  });

  it("非法输入返回 null", () => {
    expect(ipv4ToInt("300.1.1.1")).toBeNull();
    expect(ipv4ToInt("1.2.3")).toBeNull();
  });
});

describe("ipv6ToInt", () => {
  it("全零地址展开为 0（:: 无需特判的回归用例）", () => {
    expect(ipv6ToInt("::")).toBe(0n);
    expect(ipv6ToInt("::0")).toBe(0n);
    expect(ipv6ToInt("::%eth0")).toBe(0n);
  });

  it("缩写展开与数值计算正确", () => {
    expect(ipv6ToInt("::1")).toBe(1n);
    expect(ipv6ToInt("1::")).toBe(1n << 112n);
    // fe80::1 = fe80 * 2^112 + 1（首组权重 2^112）
    expect(ipv6ToInt("fe80::1")).toBe(0xfe80n * (1n << 112n) + 1n);
    // "::" + 7 个显式组：一个零组在前
    expect(ipv6ToInt("::1:2:3:4:5:6:7")).toBe(ipv6ToInt("0:1:2:3:4:5:6:7"));
  });

  it("非法输入返回 null", () => {
    expect(ipv6ToInt("1:2:3:4:5:6:7:8:9")).toBeNull();
    expect(ipv6ToInt("xxxx::1")).toBeNull();
  });
});

describe("CIDR 范围与成员判断", () => {
  it("IPv4 CIDR 计算 network/broadcast", () => {
    expect(ipv4CidrToRange("192.168.1.0/24")).toEqual({
      start: ipv4ToInt("192.168.1.0"),
      end: ipv4ToInt("192.168.1.255")
    });
    // 非整八位掩码：/22 跨 4 个 C 段
    expect(ipv4CidrToRange("192.168.0.0/22")).toEqual({
      start: ipv4ToInt("192.168.0.0"),
      end: ipv4ToInt("192.168.3.255")
    });
    expect(ipv4CidrToRange("10.1.2.3/32")).toEqual({
      start: ipv4ToInt("10.1.2.3"),
      end: ipv4ToInt("10.1.2.3")
    });
  });

  it("IPv6 CIDR 计算", () => {
    expect(ipv6CidrToRange("fe80::/16")).toEqual({
      start: ipv6ToInt("fe80::"),
      end: ipv6ToInt("fe80:ffff:ffff:ffff:ffff:ffff:ffff:ffff")
    });
    expect(ipv6CidrToRange("2001:db8::/128")).toEqual({
      start: ipv6ToInt("2001:db8::"),
      end: ipv6ToInt("2001:db8::")
    });
  });

  it("非法 CIDR 返回 null", () => {
    expect(ipv4CidrToRange("192.168.1.0/33")).toBeNull();
    expect(ipv4CidrToRange("192.168.1.0/x")).toBeNull();
    expect(ipv6CidrToRange("fe80::/129")).toBeNull();
  });

  it("isIpInCidr 按地址族判断成员", () => {
    expect(isIpInCidr("192.168.1.77", "192.168.1.0/24")).toBe(true);
    expect(isIpInCidr("192.168.2.1", "192.168.1.0/24")).toBe(false);
    expect(isIpInCidr("fe80::5", "fe80::/16")).toBe(true);
    expect(isIpInCidr("2001:db8::1", "fe80::/16")).toBe(false);
    // 空 CIDR 视为不限网段（房间未配置网段时的放行语义）
    expect(isIpInCidr("192.168.1.1", "")).toBe(true);
    // IP 本身非法时不得误判为在网段内
    expect(isIpInCidr("999.1.1.1", "192.168.1.0/24")).toBe(false);
  });
});
