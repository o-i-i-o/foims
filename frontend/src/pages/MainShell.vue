<script setup lang="ts">
/**
 * 主界面壳（移植自旧版 web/static/main.html 骨架 + navigation.js +
 * authManager.js 壳层逻辑）：侧边栏（收起/展开、RBAC 菜单隐藏、用户信息、
 * 语言切换、修改密码、退出登录）+ 主内容 RouterView。
 *
 * 分区内容页由 RouterView 按路由懒加载渲染，侧边栏视觉与交互样式复用旧版
 * layouts/sidebar.css（@layer layouts）。
 */
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink, RouterView } from "vue-router";
import { apiGet, redirectToLogin } from "../api/client";
import { useAuthStore } from "../stores/auth";
import { NAV_ITEMS } from "../router/index";
import { initTooltip } from "../composables/useTooltip";
import { usePageTimeout } from "../composables/usePageTimeout";
import AppToast from "../components/AppToast.vue";
import ChangePasswordModal from "../components/ChangePasswordModal.vue";
import LanguageMenu from "../components/LanguageMenu.vue";
import NavIcon from "../components/NavIcon.vue";

const { t } = useI18n();
const auth = useAuthStore();

// ==================== 侧边栏收起/展开 ====================

const SIDEBAR_COLLAPSE_KEY = "sidebar-collapsed";

function readCollapsedState(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSE_KEY) === "1";
  } catch {
    return false;
  }
}

const collapsed = ref(readCollapsedState());

function applyCollapsedState(value: boolean) {
  collapsed.value = value;
  // 收起样式由 body.sidebar-collapsed 驱动（布局层 CSS 按旧版结构编写）
  document.body.classList.toggle("sidebar-collapsed", value);
  try {
    localStorage.setItem(SIDEBAR_COLLAPSE_KEY, value ? "1" : "0");
  } catch {
    // 存储被禁用时仅保留会话内状态
  }
}

const toggleLabel = computed(() => t(collapsed.value ? "nav.expand_sidebar" : "nav.collapse_sidebar"));

// 空闲超时看护（等保：超时尝试续期，失败回登录页）
usePageTimeout();

// ==================== 导航与壳层动作 ====================

/** 按角色过滤可见分区（RBAC；路由守卫兜底重定向） */
const visibleNavItems = computed(() => NAV_ITEMS.filter((item) => !auth.hiddenSections.includes(item.name)));

const showChangePassword = ref(false);

/** 登录态复核（移植自旧版 checkLoginStatus）：失败一律回登录页 */
async function verifySession() {
  if (!auth.user) {
    return;
  }
  try {
    const result = await apiGet("/api/auth/me");
    if (!result.success) {
      redirectToLogin();
    }
  } catch (error) {
    console.error("检查登录状态失败:", error);
    redirectToLogin();
  }
}

onMounted(() => {
  // 恢复持久化的收起状态
  document.body.classList.toggle("sidebar-collapsed", collapsed.value);
  initTooltip();
  verifySession();
});
</script>

<template>
  <!-- 根节点不得复用 id="app"：该 id 已属 main.html 挂载容器，
       模板根再占用会在 DOM 里形成嵌套重复 id，污染 querySelector("#app") 语义 -->
  <div>
    <aside id="app-sidebar" class="sidebar" role="navigation" aria-label="Main Navigation">
      <header class="sidebar-header">
        <button
          id="sidebar-toggle-btn"
          type="button"
          class="sidebar-toggle"
          aria-controls="app-sidebar"
          :aria-expanded="collapsed ? 'false' : 'true'"
          :aria-label="toggleLabel"
          :data-tooltip="toggleLabel"
          @click="applyCollapsedState(!collapsed)"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <polyline points="15 18 9 12 15 6" />
          </svg>
        </button>
        <h1 class="sidebar-title">FOIMS</h1>
      </header>

      <nav>
        <ul class="sidebar-menu" role="menubar">
          <li v-for="item in visibleNavItems" :key="item.name" role="none">
            <RouterLink
              :to="{ name: item.name }"
              class="nav-link"
              :class="`nav-${item.name}`"
              role="menuitem"
              active-class="active"
              :data-tooltip="t(item.title)"
            >
              <span class="nav-icon" aria-hidden="true"><NavIcon :name="item.icon" /></span>
              <span class="nav-text">{{ t(item.title) }}</span>
            </RouterLink>
          </li>
        </ul>
      </nav>

      <footer class="sidebar-footer">
        <div
          class="user-info"
          :data-tooltip="auth.displayName"
          :aria-label="t('common.current_user')"
        >
          <span class="nav-icon" aria-hidden="true"><NavIcon name="user" /></span>
          <span class="nav-text" aria-live="polite">{{ auth.displayName }}</span>
        </div>

        <LanguageMenu />

        <button
          id="change-password-btn"
          type="button"
          class="footer-btn"
          :aria-label="t('auth.change_password')"
          :data-tooltip="t('auth.change_password')"
          @click="showChangePassword = true"
        >
          <span class="nav-icon" aria-hidden="true"><NavIcon name="password" /></span>
          <span class="nav-text">{{ t("auth.change_password") }}</span>
        </button>

        <button
          id="logout-btn"
          type="button"
          class="footer-btn"
          :aria-label="t('auth.logout')"
          :data-tooltip="t('auth.logout')"
          @click="auth.logout()"
        >
          <span class="nav-icon" aria-hidden="true"><NavIcon name="logout" /></span>
          <span class="nav-text">{{ t("auth.logout") }}</span>
        </button>
      </footer>
    </aside>

    <main class="main-content" role="main">
      <!-- 分区组件离场即销毁：画布状态由各页面自行持有，重新进入时重新加载 -->
      <RouterView />
    </main>

    <AppToast />
    <ChangePasswordModal v-if="showChangePassword" @close="showChangePassword = false" />
  </div>
</template>
