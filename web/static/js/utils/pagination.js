import { t } from "./i18n.js";

const DEFAULT_PAGE_SIZE_OPTIONS = [10, 20, 50, 100];

/**
 * Render a pagination control into `container`.
 *
 * @param {HTMLElement} container
 * @param {number} currentPage
 * @param {number} totalPages
 * @param {(page:number)=>void} onPageChange
 * @param {number} total
 * @param {object} [options] - optional page-size selector / jump box config
 *   { pageSize, pageSizeOptions:number[], onPageSizeChange:(size)=>void, showJump:boolean }
 */
export function renderPagination(
  container,
  currentPage,
  totalPages,
  onPageChange,
  total,
  options = {}
) {
  if (!container) {
    return;
  }

  const pagination = document.createElement("div");
  pagination.className = "pagination";

  const prevDisabled = currentPage <= 1 ? "disabled" : "";
  const nextDisabled = currentPage >= totalPages ? "disabled" : "";

  const pageNumbers = generatePageNumbers(currentPage, totalPages);

  const pageButtonsHtml = pageNumbers
    .map((page) => {
      if (page === "...") {
        return '<span class="pagination-ellipsis">...</span>';
      }
      const activeClass = page === currentPage ? "active" : "";
      return `<button class="pagination-page ${activeClass}" data-page="${page}">${page}</button>`;
    })
    .join("");

  const showSizeSelector = typeof options.onPageSizeChange === "function";
  const pageSizeOptions =
    options.pageSizeOptions && options.pageSizeOptions.length
      ? options.pageSizeOptions
      : DEFAULT_PAGE_SIZE_OPTIONS;
  const currentPageSize = options.pageSize || pageSizeOptions[0];

  const sizeSelectorHtml = showSizeSelector
    ? buildPageSizeSelector(pageSizeOptions, currentPageSize, totalPages)
    : "";
  // A jump-to-page box is only useful when there are several pages.
  const showJump = totalPages > 7;
  const jumpHtml = showJump ? buildJumpBox(currentPage, totalPages) : "";

  pagination.innerHTML = `
        <div class="pagination-left">
            <button class="pagination-btn pagination-prev" data-page="${currentPage - 1}" ${prevDisabled}>
                ${t("common.prev_page")}
            </button>
            <div class="pagination-pages">${pageButtonsHtml}</div>
            <button class="pagination-btn pagination-next" data-page="${currentPage + 1}" ${nextDisabled}>
                ${t("common.next_page")}
            </button>
            ${jumpHtml}
        </div>
        <div class="pagination-right">
            <span class="pagination-info">${t("common.page_info", { total: total || 0, page: currentPage, total_pages: totalPages })}</span>
            ${sizeSelectorHtml}
        </div>
    `;

  pagination.querySelectorAll(".pagination-btn, .pagination-page").forEach((btn) => {
    btn.addEventListener("click", () => {
      const page = parseInt(btn.dataset.page);
      if (page >= 1 && page <= totalPages && onPageChange) {
        onPageChange(page);
      }
    });
  });

  if (showSizeSelector) {
    const sizeSelect = pagination.querySelector(".pagination-size-selector");
    if (sizeSelect) {
      sizeSelect.addEventListener("change", () => {
        const size = parseInt(sizeSelect.value);
        if (Number.isFinite(size) && options.onPageSizeChange) {
          options.onPageSizeChange(size);
        }
      });
    }
  }

  if (showJump) {
    const jumpInput = pagination.querySelector(".pagination-jump-input");
    const jumpBtn = pagination.querySelector(".pagination-jump-btn");
    const doJump = () => {
      const page = parseInt(jumpInput.value);
      if (Number.isFinite(page) && page >= 1 && page <= totalPages && onPageChange) {
        onPageChange(page);
      }
    };
    if (jumpBtn) {
      jumpBtn.addEventListener("click", doJump);
    }
    if (jumpInput) {
      jumpInput.addEventListener("keydown", (e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          doJump();
        }
      });
      jumpInput.setAttribute("max", totalPages);
    }
  }

  container.innerHTML = "";
  container.appendChild(pagination);
}

function buildPageSizeSelector(options, current, _totalPages) {
  // t() 第二参是插值对象而非兜底文案,此处仅传键
  const opts = options
    .map(
      (size) =>
        `<option value="${size}" ${size === current ? "selected" : ""}>${size} / ${t("common.page")}</option>`
    )
    .join("");
  return `<span class="pagination-size">
        <select class="pagination-size-selector" title="${t("common.page_size")}">${opts}</select>
    </span>`;
}

function buildJumpBox(currentPage, totalPages) {
  return `<span class="pagination-jump">
        <span class="pagination-jump-label">${t("common.goto")}</span>
        <input type="number" class="pagination-jump-input" min="1" max="${totalPages}" value="${currentPage}" />
        <span class="pagination-jump-label">${t("common.page")}</span>
        <button class="pagination-btn pagination-jump-btn" type="button">${t("common.confirm")}</button>
    </span>`;
}

function generatePageNumbers(currentPage, totalPages) {
  const pages = [];
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
