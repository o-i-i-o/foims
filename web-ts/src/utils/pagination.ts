import { t } from "./i18n.js";

export function renderPagination(
  container: HTMLElement,
  currentPage: number,
  totalPages: number,
  onPageChange: (page: number) => void,
  total: number,
): void {
  if (!container) return;

  if (totalPages <= 1) {
    container.innerHTML = "";
    return;
  }

  const pagination = document.createElement("div");
  pagination.className = "pagination";

  const prevDisabled = currentPage <= 1 ? "disabled" : "";
  const nextDisabled = currentPage >= totalPages ? "disabled" : "";

  const pageNumbers = generatePageNumbers(currentPage, totalPages);

  const pageButtonsHtml = pageNumbers.map(page => {
    if (page === "...") {
      return `<span class="pagination-ellipsis">...</span>`;
    }
    const activeClass = page === currentPage ? "active" : "";
    return `<button class="pagination-page ${activeClass}" data-page="${page}">${page}</button>`;
  }).join("");

  pagination.innerHTML = `
        <div class="pagination-left">
            <button class="pagination-btn pagination-prev" data-page="${currentPage - 1}" ${prevDisabled}>
                ${t("common.prev_page")}
            </button>
            <div class="pagination-pages">${pageButtonsHtml}</div>
            <button class="pagination-btn pagination-next" data-page="${currentPage + 1}" ${nextDisabled}>
                ${t("common.next_page")}
            </button>
        </div>
        <div class="pagination-right">
            <span class="pagination-info">${t("common.page_info", { total: String(total || 0), page: String(currentPage), total_pages: String(totalPages) })}</span>
        </div>
    `;

  pagination.querySelectorAll(".pagination-btn, .pagination-page").forEach(btn => {
    const button = btn as HTMLButtonElement;
    button.addEventListener("click", () => {
      const page = parseInt(button.dataset.page || "0");
      if (page >= 1 && page <= totalPages && onPageChange) {
        onPageChange(page);
      }
    });
  });

  container.innerHTML = "";
  container.appendChild(pagination);
}

function generatePageNumbers(currentPage: number, totalPages: number): (number | "...")[] {
  const pages: (number | "...")[] = [];
  const delta = 2;

  if (totalPages <= 7) {
    for (let i = 1; i <= totalPages; i++) {
      pages.push(i);
    }
  } else {
    pages.push(1);

    if (currentPage > delta + 1) {
      pages.push("...");
    }

    const start = Math.max(2, currentPage - delta);
    const end = Math.min(totalPages - 1, currentPage + delta);

    for (let i = start; i <= end; i++) {
      pages.push(i);
    }

    if (currentPage < totalPages - delta - 1) {
      pages.push("...");
    }

    pages.push(totalPages);
  }

  return pages;
}

export function renderPageInfo(container: HTMLElement, currentPage: number, total: number, pageSize: number): void {
  if (!container) return;

  const totalPages = Math.ceil(total / pageSize);
  container.innerHTML = t("common.page_info", {
    total: String(total),
    page: String(currentPage),
    total_pages: String(totalPages),
  });
}

export function createPaginationState(pageSize = 20) {
  let currentPage = 1;
  let total = 0;

  return {
    getPage(): number { return currentPage; },
    getTotal(): number { return total; },
    getTotalPages(): number { return Math.ceil(total / pageSize); },
    getPageSize(): number { return pageSize; },
    getOffset(): number { return (currentPage - 1) * pageSize; },

    setPage(page: number) {
      currentPage = Math.max(1, page);
      return this;
    },
    setTotal(t: number) {
      total = t;
      return this;
    },

    next(): boolean {
      if (currentPage < this.getTotalPages()) {
        currentPage++;
        return true;
      }
      return false;
    },

    prev(): boolean {
      if (currentPage > 1) {
        currentPage--;
        return true;
      }
      return false;
    },

    first() {
      currentPage = 1;
      return this;
    },

    last() {
      currentPage = this.getTotalPages();
      return this;
    },

    getQueryParams(): Record<string, number> {
      return {
        page: currentPage,
        page_size: pageSize,
      };
    },
  };
}
