export type TemplateData = Record<string, unknown>;

export interface TemplateCache {
  [key: string]: string;
}

class TemplateManager {
  private cache: TemplateCache = {};
  private templatePath = "/static/templates";

  async loadTemplate(name: string): Promise<string> {
    if (this.cache[name]) {
      return this.cache[name];
    }

    try {
      const response = await fetch(`${this.templatePath}/${name}.html`);
      if (!response.ok) {
        throw new Error(`Failed to load template: ${name}`);
      }
      const template = await response.text();
      this.cache[name] = template;
      return template;
    } catch (error) {
      console.error(`Error loading template ${name}:`, error);
      return "";
    }
  }

  render(template: string, data: TemplateData): string {
    return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => {
      const value = data[key];
      if (value === undefined || value === null) {
        return "";
      }
      return this.escapeHtml(String(value));
    });
  }

  renderList(template: string, items: TemplateData[]): string {
    return items.map(item => this.render(template, item)).join("");
  }

  renderWithCondition(template: string, data: TemplateData): string {
    return template.replace(/\{\{#if\s+(\w+)\}\}([\s\S]*?)\{\{\/if\}\}/g, (_, key: string, content: string) => {
      return data[key] ? content : "";
    }).replace(/\{\{(\w+)\}\}/g, (_, key: string) => {
      const value = data[key];
      if (value === undefined || value === null) {
        return "";
      }
      return this.escapeHtml(String(value));
    });
  }

  renderTableRows(template: string, items: TemplateData[], startIndex = 0): string {
    return items.map((item, index) => {
      const dataWithIndex = { ...item, _index: startIndex + index + 1 };
      return this.render(template, dataWithIndex);
    }).join("");
  }

  private escapeHtml(str: string): string {
    const htmlEntities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    };
    return str.replace(/[&<>"']/g, char => htmlEntities[char] || char);
  }

  clearCache(): void {
    this.cache = {};
  }

  hasCached(name: string): boolean {
    return name in this.cache;
  }

  setTemplate(name: string, template: string): void {
    this.cache[name] = template;
  }

  getTemplate(name: string): string | undefined {
    return this.cache[name];
  }
}

export const templateManager = new TemplateManager();

export function createRowTemplate(columns: { field: string; className?: string }[]): string {
  return columns.map(col => {
    const cls = col.className ? ` class="${col.className}"` : "";
    return `<td${cls}>{{${col.field}}}</td>`;
  }).join("");
}

export function createButtonTemplate(buttons: { class: string; label: string; dataId?: boolean }[]): string {
  return buttons.map(btn => {
    const dataIdAttr = btn.dataId ? ` data-id="{{id}}"` : "";
    return `<button class="${btn.class}"${dataIdAttr}>${btn.label}</button>`;
  }).join(" ");
}

export const commonTemplates = {
  tableEmptyRow: '<tr class="empty-row"><td colspan="{{colspan}}" class="text-center">{{message}}</td></tr>',

  tableRow: "<tr>{{content}}</tr>",

  pagination: `
    <div class="pagination">
      <button class="btn btn-sm" data-page="prev">上一页</button>
      <span class="page-info">第 {{currentPage}} 页 / 共 {{totalPages}} 页</span>
      <button class="btn btn-sm" data-page="next">下一页</button>
    </div>
  `,

  statusBadge: '<span class="status-badge status-{{status}}">{{label}}</span>',

  actionButtons: `
    <button class="btn btn-sm btn-edit" data-id="{{id}}">编辑</button>
    <button class="btn btn-sm btn-delete" data-id="{{id}}">删除</button>
  `,

  selectOption: '<option value="{{value}}">{{label}}</option>',

  selectOptionGroup: `
    <optgroup label="{{label}}">
      {{options}}
    </optgroup>
  `,
};

export function renderEmptyRow(colspan: number, message = "暂无数据"): string {
  return templateManager.render(commonTemplates.tableEmptyRow, { colspan, message });
}

export function renderStatusBadge(status: string, label: string): string {
  return templateManager.render(commonTemplates.statusBadge, { status, label });
}

export function renderActionButtons(id: string | number): string {
  return templateManager.render(commonTemplates.actionButtons, { id });
}
