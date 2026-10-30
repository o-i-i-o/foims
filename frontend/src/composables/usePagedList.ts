/**
 * 分页列表与表头排序公共工具（自 ResourcesPage 抽出，供多分区页面复用）：
 * usePagedList 工厂（排序 + 越界回退 + 可变页大小 + 翻页占位/过期请求取消）、跨页连续序号、
 * 表头三态类与点击排序守卫（点击落在输入控件内时不触发排序）。
 */
import { computed, reactive, ref, watch, type Ref } from "vue";
import { useI18n } from "vue-i18n";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet } from "../api/client";
import type { Paged } from "../types/api";

export const PAGE_SIZE_OPTIONS: number[] = [10, 20, 50, 100];

/** 预取幂等窗口：窗口内重复 hover 同一页命中缓存不重发预取请求（注：点击翻页后
 * 本体查询按自身新鲜度策略判定刷新，该值不参与其判定——observer 侧读自身 staleTime） */
const PREFETCH_STALE_TIME = 30_000;

export interface UsePagedListOptions {
  /**
   * 查询键前缀（分区标识）：传 utils/queryKeys 中的常量数组，
   * 与失效方共用同一引用，避免 key 分裂；单处私用的标签可继续传字符串
   */
  tag: string | readonly string[];
  defaultPageSize: number;
  /** 查询启用条件（页面切换懒加载） */
  enabled?: Ref<boolean> | boolean;
  /** 组装分页请求 URL（页码/页大小/排序字段/排序方向） */
  buildUrl: (page: number, pageSize: number, sortBy: string, sortOrder: string) => string;
  /** 附加查询键（筛选参数变化触发重新查询） */
  extraKey?: Ref<Record<string, unknown>>;
  defaultSortBy?: string;
  defaultSortOrder?: "asc" | "desc";
  /** 自动轮询周期（毫秒）；窗口失焦时 TanStack 默认暂停（主机监控页用） */
  refetchInterval?: number;
}

/**
 * 表头排序工具的最小依赖：分页列表与无分页列表（如定时任务）共用的形态，
 * sortClass/onThSort 只消费 sort 与 toggleSort，不关心分页字段。
 */
export interface SortableTable {
  sort: { by: string; order: "asc" | "desc" };
  toggleSort: (key: string) => void;
}

/**
 * usePagedList 返回的列表状态（reactive 解包后的形状），
 * 供 seqIndex/sortClass/onThSort 等辅助函数引用。
 */
export interface PagedList<T> extends SortableTable {
  page: number;
  pageSize: number;
  pageSizeOptions: number[];
  changePageSize: (size: number) => void;
  items: T[];
  total: number;
  totalPages: number;
  isError: boolean;
  errorMessage: string;
  /** 手动重拉当前页（刷新按钮；不重置页码/筛选） */
  refetch: () => void;
  /** 悬停预取目标页（Pagination hover 事件接线；越界/当前页静默忽略） */
  prefetch: (target: number) => void;
}

/**
 * 分页列表封装：默认 created_at 倒序（需按名称等字段排序的列表经 defaultSort
 * 指定），排序/页大小切换重置页码；空页且页码越界（末页数据删除后）自动
 * 回退最后一页。
 */
export function usePagedList<T>(options: UsePagedListOptions): PagedList<T> {
  const {
    tag,
    defaultPageSize,
    enabled,
    buildUrl,
    extraKey,
    defaultSortBy = "created_at",
    defaultSortOrder = "desc",
    refetchInterval,
  } = options;
  const { t } = useI18n();
  const queryClient = useQueryClient();
  // 前缀展开一次：tag 不随渲染变化，无需响应式
  const tagParts: string[] = typeof tag === "string" ? [tag] : [...tag];
  const page = ref(1);
  const pageSize = ref(defaultPageSize);
  const sort = reactive({ by: defaultSortBy, order: defaultSortOrder });

  /** 查询键组装：useQuery 与 prefetch 共用同一形态，避免 key 分裂导致失效/预取打不中 */
  const buildKey = (targetPage: number): unknown[] => [
    ...tagParts,
    {
      page: targetPage,
      pageSize: pageSize.value,
      sortBy: sort.by,
      sortOrder: sort.order,
      ...extraKey?.value
    }
  ];

  /** 查询体：useQuery 与 prefetch 共用（signal 透传与取消归一） */
  const fetchPage = async (targetPage: number, signal?: AbortSignal): Promise<Paged<T>> => {
    // 透传 TanStack 取消信号：key 变化（快速翻页/切筛选）时中止过期请求
    const result = await apiGet<Paged<T>>(
      buildUrl(targetPage, pageSize.value, sort.by, sort.order),
      signal ? { signal } : {}
    );
    if (result.errorType === "request_cancelled") {
      // 过期请求已被新查询取消：按 AbortError 抛出静默丢弃，不计入列表错误态
      throw new DOMException("aborted", "AbortError");
    }
    if (!result.success || !result.data) {
      // apiClient 已将失败归一为 { success:false } 不抛异常，
      // 此处显式抛出让查询进入 error 态（isError/error.message 可渲染）
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  };

  const query = useQuery({
    queryKey: computed(() => buildKey(page.value)),
    queryFn: ({ signal }) => fetchPage(page.value, signal),
    // 翻页/排序/筛选期间保留旧页数据渲染，避免整表闪空（isPlaceholderData 可判占位态）
    placeholderData: keepPreviousData,
    enabled,
    refetchInterval
  });

  /** 末页号：越界回退 watch 与 prefetch 守卫共用 */
  const totalPages = computed(() => query.data.value?.total_pages ?? 1);

  watch(
    () => query.data.value,
    (data) => {
      if (!data || !Array.isArray(data.items) || data.items.length !== 0 || page.value <= 1) {
        return;
      }
      const lastPage = data.total_pages;
      if (lastPage > 0 && page.value > lastPage) {
        page.value = lastPage;
      }
    }
  );

  function toggleSort(key: string): void {
    if (sort.by === key) {
      sort.order = sort.order === "asc" ? "desc" : "asc";
    } else {
      sort.by = key;
      sort.order = "asc";
    }
    page.value = 1;
  }

  function changePageSize(size: number): void {
    pageSize.value = size;
    page.value = 1;
  }

  return reactive({
    page,
    pageSize,
    sort,
    toggleSort,
    changePageSize,
    pageSizeOptions: PAGE_SIZE_OPTIONS,
    items: computed(() => (Array.isArray(query.data.value?.items) ? query.data.value.items : [])),
    total: computed(() => query.data.value?.total ?? 0),
    totalPages,
    isError: computed(() => query.isError.value),
    errorMessage: computed(() => query.error.value?.message || t("common.load_failed")),
    refetch: () => {
      void query.refetch();
    },
    prefetch: (target: number) => {
      if (target < 1 || target === page.value || target > totalPages.value) {
        return;
      }
      // 提前发起请求：点击翻页时数据已在缓存直出（配合 keepPreviousData 无闪空）；
      // staleTime 仅保证窗口内重复 hover 幂等防重发，点击后本体查询仍会校验刷新
      void queryClient.prefetchQuery({
        queryKey: buildKey(target),
        queryFn: ({ signal }) => fetchPage(target, signal),
        staleTime: PREFETCH_STALE_TIME
      });
    }
  }) as PagedList<T>;
}

/** 序号列：跨页连续（(page-1)*pageSize + 行内序） */
export function seqIndex<T>(list: PagedList<T>, index: number): number {
  return (list.page - 1) * list.pageSize + index + 1;
}

/** 排序列头公共类绑定（sorted/asc/desc 三态，与 LogsPage 表头一致） */
export function sortClass(list: SortableTable, key: string): Record<string, boolean> {
  return {
    sorted: list.sort.by === key,
    asc: list.sort.by === key && list.sort.order === "asc",
    desc: list.sort.by === key && list.sort.order === "desc"
  };
}

/**
 * 表头点击排序：表头内嵌过滤输入框、搜索弹层等控件时不触发排序
 * （closest 守卫）
 */
export function onThSort(event: MouseEvent, list: SortableTable, key: string): void {
  if (event.target instanceof Element && event.target.closest("input, select, textarea, button, a, .th-search-popover")) {
    return;
  }
  list.toggleSort(key);
}
