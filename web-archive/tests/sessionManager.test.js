// 会话存储管理单元测试（utils/sessionManager.js）：
// 核心行为是 rememberMe 决定会话落在 localStorage 还是 sessionStorage
import { SessionManager } from "../static/js/utils/sessionManager.js";

const flushStorage = () => {
  localStorage.clear();
  sessionStorage.clear();
};

describe("SessionManager", () => {
  beforeEach(flushStorage);

  it("未登录态：hasSession 为 false，getUser 为 null", () => {
    expect(SessionManager.hasSession()).toBe(false);
    expect(SessionManager.getUser()).toBeNull();
  });

  it("默认写入 sessionStorage（关闭标签即失效）", () => {
    SessionManager.setUser({ id: 1, username: "admin" });
    expect(SessionManager.hasSession()).toBe(true);
    expect(sessionStorage.getItem("user")).toContain("admin");
    expect(localStorage.getItem("user")).toBeNull();
  });

  it("rememberMe 写入 localStorage（跨会话保留）", () => {
    SessionManager.setUser({ id: 1, username: "admin" }, true);
    expect(localStorage.getItem("user")).toContain("admin");
    expect(sessionStorage.getItem("user")).toBeNull();
    expect(SessionManager.isRememberMe()).toBe(true);
  });

  it("getUser 返回反序列化对象", () => {
    SessionManager.setUser({ id: 7, username: "oi", role: "admin" }, true);
    expect(SessionManager.getUser()).toEqual({ id: 7, username: "oi", role: "admin" });
  });

  it("updateUser 浅合并更新字段", () => {
    SessionManager.setUser({ id: 7, username: "oi", role: "user" });
    const updated = SessionManager.updateUser({ role: "admin" });
    expect(updated).toEqual({ id: 7, username: "oi", role: "admin" });
    expect(SessionManager.getUser().role).toBe("admin");
  });

  it("未登录时 updateUser 返回 null", () => {
    expect(SessionManager.updateUser({ role: "admin" })).toBeNull();
  });

  it("clear 同时清理两种存储与 rememberMe 标志", () => {
    SessionManager.setUser({ id: 1 }, true);
    SessionManager.clear();
    expect(localStorage.getItem("user")).toBeNull();
    expect(localStorage.getItem("rememberMe")).toBeNull();
    expect(sessionStorage.getItem("user")).toBeNull();
    expect(SessionManager.hasSession()).toBe(false);
  });

  it("损坏的 JSON 回退为未登录而不是抛错", () => {
    localStorage.setItem("user", "{broken json");
    expect(SessionManager.getUser()).toBeNull();
    expect(SessionManager.hasSession()).toBe(false);
  });
});
