// escapeHtml 与子标签持久化的单元测试（utils/helpers.js）
import { escapeHtml, getActiveSubtab, setActiveSubtab } from "../static/js/utils/helpers.js";

describe("escapeHtml", () => {
  it("完整转义五个敏感字符（含属性场景的引号）", () => {
    expect(escapeHtml(`<a href="x">&'</a>`)).toBe(
      "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
    );
  });

  it.each([
    ["&", "&amp;"],
    ["<", "&lt;"],
    [">", "&gt;"],
    ['"', "&quot;"],
    ["'", "&#39;"]
  ])("单字符 %s → %s", (input, expected) => {
    expect(escapeHtml(input)).toBe(expected);
  });

  it("空值与数字入参安全返回", () => {
    expect(escapeHtml("")).toBe("");
    expect(escapeHtml(null)).toBe("");
    expect(escapeHtml(0)).toBe("");
    // 非空数字转字符串处理
    expect(escapeHtml(42)).toBe("42");
  });

  it("中文与常规文本保持原样", () => {
    expect(escapeHtml("机柜 A-01（三层）")).toBe("机柜 A-01（三层）");
  });
});

describe("子标签持久化", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("保存后可读取，键按页面隔离", () => {
    setActiveSubtab("resources", "devices");
    setActiveSubtab("logs", "operation");
    expect(getActiveSubtab("resources")).toBe("devices");
    expect(getActiveSubtab("logs")).toBe("operation");
  });

  it("覆盖写入以最后一次为准", () => {
    setActiveSubtab("resources", "devices");
    setActiveSubtab("resources", "networks");
    expect(getActiveSubtab("resources")).toBe("networks");
  });

  it("缺参不写入，未记录返回 null", () => {
    setActiveSubtab("", "devices");
    setActiveSubtab("resources", "");
    expect(getActiveSubtab("resources")).toBeNull();
    expect(getActiveSubtab("")).toBeNull();
    expect(getActiveSubtab("unknown-page")).toBeNull();
  });
});
