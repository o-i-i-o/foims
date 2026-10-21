<script setup lang="ts">
/**
 * 侧边栏语言选择器（移植自旧版 languageMenu.js）：按钮 + body 级弹出菜单。
 * 语言列表固定为已迁移的 zh/en（native_name 直接维护于此，新增语言需
 * 补齐语言包并在此登记）。展开时右对齐按钮、页脚向上弹出，收起态
 * （窄图标栏）改为从右侧飞出；视口/滚动变化后直接关闭。
 */
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { changeLanguage } from "../i18n/index";
import NavIcon from "./NavIcon.vue";

const { t, locale } = useI18n();

// as const 使其为元组类型：LANGUAGES[0] 索引访问不会因 noUncheckedIndexedAccess 变宽
const LANGUAGES = [
  { code: "zh", nativeName: "中文" },
  { code: "en", nativeName: "English" }
] as const;

const btnRef = ref<HTMLElement | null>(null);
const menuRef = ref<HTMLElement | null>(null);
const open = ref(false);
const menuStyle = ref<Record<string, string>>({});

const currentName = computed(
  () => LANGUAGES.find((item) => item.code === locale.value)?.nativeName ?? LANGUAGES[0].nativeName
);

function positionMenu() {
  const btn = btnRef.value;
  const menu = menuRef.value;
  if (!btn || !menu) {
    return;
  }

  const rect = btn.getBoundingClientRect();
  const menuWidth = menu.offsetWidth;
  const menuHeight = menu.offsetHeight;

  // 收起态从窄栏右侧飞出；展开态右对齐按钮
  let left = document.body.classList.contains("sidebar-collapsed") ? rect.right + 8 : rect.right - menuWidth;
  left = Math.max(8, Math.min(left, window.innerWidth - menuWidth - 8));

  // 页脚位于底部，默认向上弹出，空间不足时向下
  let top = rect.top - menuHeight - 8;
  if (top < 8) {
    top = rect.bottom + 8;
  }

  menuStyle.value = { left: `${left}px`, top: `${top}px` };
}

function toggleMenu() {
  if (open.value) {
    closeMenu();
    return;
  }
  open.value = true;
  // 菜单先挂载才能量取尺寸，DOM 更新后（绘制前）定位，无闪烁
  nextTick(positionMenu);
}

function closeMenu() {
  open.value = false;
}

function selectLanguage(code: string) {
  if (code !== locale.value) {
    changeLanguage(code);
  }
  closeMenu();
}

function onDocumentClick(e: Event) {
  const target = e.target;
  if (open.value && target instanceof Node && !menuRef.value?.contains(target) && !btnRef.value?.contains(target)) {
    closeMenu();
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    closeMenu();
  }
}

// 视口变化后定位失效，直接关闭
function onViewportChange() {
  closeMenu();
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
  document.addEventListener("keydown", onKeydown);
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);
});

onUnmounted(() => {
  document.removeEventListener("click", onDocumentClick);
  document.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("scroll", onViewportChange, true);
});
</script>

<template>
  <button
    id="language-selector"
    ref="btnRef"
    type="button"
    class="footer-btn"
    aria-haspopup="menu"
    :aria-expanded="open ? 'true' : 'false'"
    :aria-label="t('language.toggle')"
    :data-tooltip="t('language.title')"
    @click="toggleMenu"
  >
    <span class="nav-icon lang-icon" aria-hidden="true"><NavIcon name="language" /></span>
    <span class="lang-name">{{ currentName }}</span>
  </button>

  <Teleport to="body">
    <div v-if="open" ref="menuRef" class="lang-menu" role="menu" :aria-label="t('language.title')" :style="menuStyle">
      <button
        v-for="language in LANGUAGES"
        :key="language.code"
        type="button"
        class="lang-menu-item"
        role="menuitemradio"
        :aria-checked="language.code === locale ? 'true' : 'false'"
        @click="selectLanguage(language.code)"
      >
        <NavIcon name="check" />
        {{ language.nativeName }}
      </button>
    </div>
  </Teleport>
</template>
