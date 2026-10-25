/**
 * 分区子页签状态：localStorage 记忆 + 路由 query 深链（分区页共用）。
 *
 * 统一语义（原四个分区页各自实现过一份）：
 * - 初始页签：`#/system?tab=users` 深链优先 → 上次记忆的页签 → 清单首项；
 * - 切换：写回 localStorage 并用 replace 更新 query（不产生历史记录）；
 * - 前进/后退：监听 query 反向同步（仅深链模式）。
 *
 * 记忆值与 query 值都必须命中页签清单，脏值（历史遗留/手改 URL）回落到默认项。
 */
import { shallowRef, watch, type Ref } from "vue";
import { useRoute, useRouter } from "vue-router";

/** 单个页签元数据 */
export interface SubTabMeta<Id extends string = string> {
  /** 页签 id（同时是 query 的 tab 值与 localStorage 记忆值） */
  id: Id;
  /** 标题 i18n 键 */
  titleKey: string;
}

export interface UseSubTabsOptions<Id extends string> {
  /** localStorage 记忆键（各分区页唯一） */
  storageKey: string;
  /** 页签清单：顺序即渲染顺序，首项为默认页签（不可为空） */
  tabs: readonly [SubTabMeta<Id>, ...SubTabMeta<Id>[]];
  /** 是否与路由 query 的 tab 参数双向同步（默认 true；可视化页签不参与深链） */
  syncQuery?: boolean;
  /** 切换到非当前页签后的附加动作（首次进入不触发） */
  onSwitch?: (tabId: Id) => void;
}

export interface SubTabs<Id extends string> {
  /** 当前页签 id（响应式） */
  activeTab: Ref<Id>;
  /** 切换页签：记忆 + 深链 + 附加动作 */
  switchTab: (tabId: Id) => void;
}

export function useSubTabs<Id extends string>(options: UseSubTabsOptions<Id>): SubTabs<Id> {
  const { storageKey, tabs, syncQuery = true, onSwitch } = options;

  // 深链模式下才接线路由：非深链页面（可视化）不产生 route 依赖
  const route = syncQuery ? useRoute() : null;
  const router = syncQuery ? useRouter() : null;

  const isTabId = (value: string): value is Id => tabs.some((tab) => tab.id === value);

  function readInitialTab(): Id {
    if (route) {
      const fromQuery = String(route.query.tab ?? "");
      if (isTabId(fromQuery)) {
        return fromQuery;
      }
    }
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved !== null && isTabId(saved)) {
        return saved;
      }
    } catch {
      // 存储被禁用时按默认页签展示
    }
    return tabs[0].id;
  }

  // 页签 id 为字符串字面量：用 shallowRef 避免 ref 的深层解包改宽泛型
  const activeTab = shallowRef(readInitialTab()) as Ref<Id>;

  function switchTab(tabId: Id): void {
    if (tabId === activeTab.value) {
      return;
    }
    activeTab.value = tabId;
    try {
      localStorage.setItem(storageKey, tabId);
    } catch {
      // 存储被禁用时仅会话内生效
    }
    if (route && router) {
      // hash query 记录页签，便于书签直达（replace 不产生历史记录）
      void router.replace({ query: { ...route.query, tab: tabId } });
    }
    onSwitch?.(tabId);
  }

  if (route) {
    // 浏览器前进/后退时按 URL 同步页签
    watch(
      () => route.query.tab,
      (value) => {
        const tabId = String(value ?? "");
        if (isTabId(tabId) && tabId !== activeTab.value) {
          activeTab.value = tabId;
        }
      }
    );
  }

  return { activeTab, switchTab };
}
