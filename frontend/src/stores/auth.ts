/**
 * 认证状态 store：包装 utils/session.js 的用户信息副本为响应式状态。
 * 真实凭证由 HttpOnly Cookie 承载，store 仅承担界面所需的用户快照
 * （角色分区可见性 / 侧边栏用户名 / 登出）。
 */
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { apiPost, redirectToLogin } from "../api/client";
import { clearSession, getUser, type UserInfo } from "../utils/session";
import { getHiddenSections, getReadOnlySections } from "../utils/roleCapabilities";
import { i18n } from "../i18n/index";

export const useAuthStore = defineStore("auth", () => {
  const user = ref<UserInfo | null>(getUser());

  const role = computed(() => user.value?.role || "user");

  /** 当前角色不可见的分区（侧边栏隐藏 + 路由守卫重定向） */
  const hiddenSections = computed(() => getHiddenSections(role.value));

  /** 当前角色以只读模式展示的分区 */
  const readOnlySections = computed(() => getReadOnlySections(role.value));

  /** 侧边栏欢迎文案（随语言切换刷新） */
  const displayName = computed(() =>
    user.value ? i18n.global.t("auth.welcome", { username: user.value.username }) : ""
  );

  /** 清空本地会话并跳转登录页 */
  function localLogout(): void {
    clearSession();
    user.value = null;
    redirectToLogin();
  }

  /** 服务端登出：吊销令牌后回登录页（请求失败也兜底回登录页） */
  async function logout(): Promise<void> {
    try {
      await apiPost("/api/auth/logout", {});
    } catch (error) {
      console.error("退出登录请求失败:", error);
    }
    localLogout();
  }

  return { user, role, hiddenSections, readOnlySections, displayName, localLogout, logout };
});
