type EventHandler = (event: Event, target: HTMLElement, data?: Record<string, string>) => void | Promise<void>;

interface DelegatedEvent {
  selector: string;
  handler: EventHandler;
  eventType: string;
}

class EventDelegator {
  private events: Map<string, DelegatedEvent[]> = new Map();
  private boundHandlers: Map<string, EventListener> = new Map();
  private dataAttributes = ["id", "action", "page", "sort", "order", "type", "target"];

  on(container: Element | Document, eventType: string, selector: string, handler: EventHandler): void {
    const containerKey = this.getContainerKey(container);
    const key = `${containerKey}:${eventType}`;

    if (!this.events.has(key)) {
      this.events.set(key, []);
      this.attachDelegatedListener(container, eventType, key);
    }

    this.events.get(key)!.push({ selector, handler, eventType });
  }

  off(container: Element | Document, eventType: string, selector: string): void {
    const containerKey = this.getContainerKey(container);
    const key = `${containerKey}:${eventType}`;
    const events = this.events.get(key);

    if (events) {
      const filtered = events.filter(e => e.selector !== selector);
      if (filtered.length === 0) {
        this.events.delete(key);
        this.detachDelegatedListener(container, eventType, key);
      } else {
        this.events.set(key, filtered);
      }
    }
  }

  once(container: Element | Document, eventType: string, selector: string, handler: EventHandler): void {
    const wrappedHandler: EventHandler = (event, target, data) => {
      this.off(container, eventType, selector);
      return handler(event, target, data);
    };
    this.on(container, eventType, selector, wrappedHandler);
  }

  trigger(element: Element, eventType: string, detail?: unknown): boolean {
    const event = new CustomEvent(eventType, {
      bubbles: true,
      cancelable: true,
      detail,
    });
    return element.dispatchEvent(event);
  }

  private attachDelegatedListener(container: Element | Document, eventType: string, key: string): void {
    const delegatedHandler = (event: Event) => {
      this.handleDelegatedEvent(event, key);
    };

    this.boundHandlers.set(key, delegatedHandler);
    container.addEventListener(eventType, delegatedHandler);
  }

  private detachDelegatedListener(container: Element | Document, eventType: string, key: string): void {
    const handler = this.boundHandlers.get(key);
    if (handler) {
      container.removeEventListener(eventType, handler);
      this.boundHandlers.delete(key);
    }
  }

  private handleDelegatedEvent(event: Event, key: string): void {
    const events = this.events.get(key);
    if (!events) return;

    const target = event.target as HTMLElement;
    if (!target) return;

    for (const { selector, handler } of events) {
      const matchedElement = target.closest(selector) as HTMLElement | null;
      if (matchedElement) {
        const data = this.extractDataAttributes(matchedElement);
        handler(event, matchedElement, data);
        break;
      }
    }
  }

  private extractDataAttributes(element: HTMLElement): Record<string, string> {
    const data: Record<string, string> = {};

    for (const attr of this.dataAttributes) {
      const value = element.getAttribute(`data-${attr}`);
      if (value !== null) {
        data[attr] = value;
      }
    }

    for (const attr of element.attributes) {
      if (attr.name.startsWith("data-") && !this.dataAttributes.includes(attr.name.slice(5))) {
        data[attr.name.slice(5)] = attr.value;
      }
    }

    return data;
  }

  private getContainerKey(container: Element | Document): string {
    if (container === document) {
      return "document";
    }
    return (container as Element).id || `container-${Math.random().toString(36).slice(2, 9)}`;
  }

  clear(): void {
    for (const [key, handler] of this.boundHandlers.entries()) {
      const [containerKey, eventType] = key.split(":");
      const container = containerKey === "document" ? document : document.getElementById(containerKey);
      if (container) {
        container.removeEventListener(eventType, handler);
      }
    }
    this.events.clear();
    this.boundHandlers.clear();
  }
}

export const eventDelegator = new EventDelegator();

export function setupTableEvents(
  container: Element | Document,
  tableSelector: string,
  handlers: {
    onEdit?: (id: string, target: HTMLElement) => void | Promise<void>;
    onDelete?: (id: string, target: HTMLElement) => void | Promise<void>;
    onCustom?: (action: string, id: string, target: HTMLElement) => void | Promise<void>;
  }
): void {
  if (handlers.onEdit) {
    eventDelegator.on(container, "click", `${tableSelector} .btn-edit`, (_event, target, data) => {
      if (data?.id) handlers.onEdit!(data.id, target);
    });
  }

  if (handlers.onDelete) {
    eventDelegator.on(container, "click", `${tableSelector} .btn-delete`, (_event, target, data) => {
      if (data?.id) handlers.onDelete!(data.id, target);
    });
  }

  if (handlers.onCustom) {
    eventDelegator.on(container, "click", `${tableSelector} [data-action]`, (_event, target, data) => {
      if (data?.action && data?.id) handlers.onCustom!(data.action, data.id, target);
    });
  }
}

export function setupFormEvents(
  container: Element | Document,
  formSelector: string,
  handlers: {
    onSubmit?: (formData: FormData, target: HTMLFormElement) => void | Promise<void>;
    onReset?: (target: HTMLFormElement) => void;
    onFieldChange?: (field: string, value: string, target: HTMLElement) => void | Promise<void>;
  }
): void {
  if (handlers.onSubmit) {
    eventDelegator.on(container, "submit", formSelector, (event) => {
      event.preventDefault();
      const form = event.target as HTMLFormElement;
      const formData = new FormData(form);
      handlers.onSubmit!(formData, form);
    });
  }

  if (handlers.onReset) {
    eventDelegator.on(container, "reset", formSelector, (event) => {
      const form = event.target as HTMLFormElement;
      handlers.onReset!(form);
    });
  }

  if (handlers.onFieldChange) {
    eventDelegator.on(container, "change", `${formSelector} input, ${formSelector} select, ${formSelector} textarea`, (_event, target) => {
      const field = target.getAttribute("name") || target.id;
      const value = (target as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement).value;
      handlers.onFieldChange!(field, value, target);
    });
  }
}

export function setupPaginationEvents(
  container: Element | Document,
  paginationSelector: string,
  onPageChange: (page: number) => void | Promise<void>
): void {
  eventDelegator.on(container, "click", `${paginationSelector} [data-page]`, (_event, target, data) => {
      const page = data?.page;
    if (page === "prev" || page === "next") {
      const currentPageEl = target.closest(paginationSelector)?.querySelector(".current-page");
      const currentPage = currentPageEl ? parseInt(currentPageEl.textContent || "1", 10) : 1;
      onPageChange(page === "prev" ? currentPage - 1 : currentPage + 1);
    } else if (page && !isNaN(parseInt(page, 10))) {
      onPageChange(parseInt(page, 10));
    }
  });
}

export function setupSearchEvents(
  container: Element | Document,
  searchSelector: string,
  onSearch: (term: string) => void | Promise<void>,
  debounceMs = 300
): void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  eventDelegator.on(container, "input", searchSelector, (_event, target) => {
    const value = (target as HTMLInputElement).value.trim();
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = setTimeout(() => onSearch(value), debounceMs);
  });

  eventDelegator.on(container, "keydown", searchSelector, (event) => {
    if ((event as KeyboardEvent).key === "Enter") {
      event.preventDefault();
      if (timeoutId) clearTimeout(timeoutId);
      const target = event.target as HTMLInputElement;
      onSearch(target.value.trim());
    }
  });
}

export function setupSortEvents(
  container: Element | Document,
  tableSelector: string,
  onSort: (field: string, order: "asc" | "desc") => void | Promise<void>
): void {
  eventDelegator.on(container, "click", `${tableSelector} th[data-sortable="true"]`, (event, target) => {
    event.preventDefault();
    const field = target.getAttribute("data-field");
    const currentOrder = target.getAttribute("data-order") as "asc" | "desc" | null;
    const newOrder: "asc" | "desc" = currentOrder === "asc" ? "desc" : "asc";

    if (field) {
      const table = target.closest("table");
      table?.querySelectorAll('th[data-sortable="true"]').forEach(th => {
        th.removeAttribute("data-order");
        th.classList.remove("sort-asc", "sort-desc");
      });

      target.setAttribute("data-order", newOrder);
      target.classList.add(`sort-${newOrder}`);
      onSort(field, newOrder);
    }
  });
}
