/**
 * 页面空闲超时（等保要求）：
 * 超时阈值取服务端配置（page_timeout，缺省 30 分钟），用户输入事件重置
 * 计时；超时后先尝试刷新令牌——成功提示会话已刷新，失败提示重新登录并
 * 跳回登录页。
 */
import { onMounted, onUnmounted } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, redirectToLogin, refreshToken } from "../api/client";
import { showToast } from "./useToast";
import type { PageTimeoutConfig } from "../types/api/generated/PageTimeoutConfig";

const ACTIVITY_EVENTS = ["mousedown", "mousemove", "keypress", "scroll", "touchstart", "click"];

/** 空闲超时缺省分钟数（服务端配置缺失/读取失败时的唯一兑底口径） */
const DEFAULT_TIMEOUT_MINUTES = 30;

/** 从服务端读取空闲超时分钟数（读取失败按默认值） */
async function loadTimeoutMinutes(): Promise<number> {
  try {
    const result = await apiGet<PageTimeoutConfig>("/api/system/page-timeout");
    if (result.success && result.data) {
      return result.data.page_timeout || DEFAULT_TIMEOUT_MINUTES;
    }
  } catch (error) {
    console.error("获取页面超时配置失败:", error);
  }
  return DEFAULT_TIMEOUT_MINUTES;
}

export function usePageTimeout(): void {
  const { t } = useI18n();

  let timeoutId: number | null = null;
  let timeoutMinutes = DEFAULT_TIMEOUT_MINUTES;

  function resetTimeout(): void {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(handleTimeout, timeoutMinutes * 60 * 1000);
  }

  async function handleTimeout(): Promise<void> {
    const refreshed = await refreshToken();

    if (refreshed) {
      showToast(t("auth.session_refreshed"), "success");
      resetTimeout();
      return;
    }

    showToast(t("auth.session_timeout"), "info");
    setTimeout(() => {
      redirectToLogin();
    }, 2000);
  }

  onMounted(async () => {
    timeoutMinutes = await loadTimeoutMinutes();
    ACTIVITY_EVENTS.forEach((event) => {
      document.addEventListener(event, resetTimeout, true);
    });
    resetTimeout();
  });

  onUnmounted(() => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    ACTIVITY_EVENTS.forEach((event) => {
      document.removeEventListener(event, resetTimeout, true);
    });
  });
}
