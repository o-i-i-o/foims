/**
 * 动态行管理器基类
 *
 * 抽取 room/cabinet 表单中多个「动态增删行」管理器的公共骨架：
 * 容器定位、空状态渲染、添加/删除行、行内事件绑定等。
 *
 * 子类需实现：
 *   - createRow(data)  构造单行 DOM（内部应调用 this.bindItemEvents(item)）
 *   - collectData()    收集全部行数据
 *   并可覆盖 addLabel() / emptyHintText() 以支持动态文案（如多语言/类型切换）。
 *
 * config 选项：
 *   containerId          容器元素 id
 *   itemSelector         单行的选择器（如 '.cabinet-position-item'）
 *   emptyClassName       空状态占位元素的 className（不含点）
 *   removeBtnSelector    行内「删除」按钮选择器
 *   addBtnSelector       行内/空状态「添加」按钮选择器（按钮模式或在行内显示添加时必填）
 *   emptyMode            'hint'（纯文字提示）| 'button'（带添加按钮）
 *   externalAddButtonId  容器外的「添加」按钮 id（如机柜底部的添加机位按钮），可选
 *   showInRowAddButton   是否在每行行内显示「添加」按钮（仅末行可见），默认 false
 */
export class DynamicRowManager {
  constructor(config) {
    this.config = config;
    this.container = null;
    this.handlers = new WeakMap();
    this.addHandler = null;
  }

  ensureContainer() {
    if (!this.container || !document.contains(this.container)) {
      this.container = document.getElementById(this.config.containerId);
    }
    return this.container;
  }

  /** 「添加」按钮文案，子类可覆盖以返回动态值（如随房间类型变化） */
  addLabel() {
    return this.config.addLabel || "";
  }

  /** 空状态文字提示，子类可覆盖（emptyMode='hint' 时使用） */
  emptyHintText() {
    return this.config.emptyHintText || "";
  }

  /** 绑定容器外的「添加」按钮（如机柜底部的添加按钮） */
  bindExternalAddButton() {
    if (!this.config.externalAddButtonId) return;
    const addBtn = document.getElementById(this.config.externalAddButtonId);
    if (!addBtn) return;
    if (this.addHandler) {
      addBtn.removeEventListener("click", this.addHandler);
    }
    this.addHandler = () => this.addItem();
    addBtn.addEventListener("click", this.addHandler);
  }

  /** 根据 item 数量显示/隐藏空状态占位 */
  updateEmptyState() {
    if (!this.ensureContainer()) return;
    const { itemSelector, emptyClassName, emptyMode, addBtnSelector } = this.config;
    const existing = this.container.querySelector(`.${emptyClassName}`);
    const items = this.container.querySelectorAll(itemSelector);
    if (items.length === 0 && !existing) {
      const emptyDiv = document.createElement("div");
      emptyDiv.className = emptyMode === "hint" ? `${emptyClassName} text-muted` : emptyClassName;
      if (emptyMode === "hint") {
        emptyDiv.textContent = this.emptyHintText();
      } else {
        const btnClass = addBtnSelector.replace(".", "");
        emptyDiv.innerHTML = `<button type="button" class="btn btn-secondary btn-sm ${btnClass}">${this.addLabel()}</button>`;
        const addBtn = emptyDiv.querySelector(addBtnSelector);
        if (addBtn) {
          const handler = () => this.addItem();
          this.handlers.set(addBtn, handler);
          addBtn.addEventListener("click", handler);
        }
      }
      this.container.appendChild(emptyDiv);
    } else if (items.length > 0 && existing) {
      existing.remove();
    }
  }

  /** 仅在末行显示「添加」按钮（showInRowAddButton 时） */
  updateAddButtons() {
    if (!this.ensureContainer() || !this.config.showInRowAddButton) return;
    const { itemSelector, addBtnSelector } = this.config;
    const items = this.container.querySelectorAll(itemSelector);
    items.forEach((item, index) => {
      const addBtn = item.querySelector(addBtnSelector);
      if (addBtn) {
        addBtn.style.display = index === items.length - 1 ? "" : "none";
        addBtn.textContent = this.addLabel();
      }
    });
  }

  /** 添加一行：移除空状态 → 构造行 → 追加 → 刷新按钮 */
  addItem(data = {}) {
    if (!this.ensureContainer()) return;
    const emptyState = this.container.querySelector(`.${this.config.emptyClassName}`);
    if (emptyState) emptyState.remove();
    const item = this.createRow(data);
    this.container.appendChild(item);
    this.updateAddButtons();
    this.updateEmptyState();
    return item;
  }

  /** 绑定行内「删除」与（可选）「添加」按钮 */
  bindItemEvents(item) {
    const removeBtn = item.querySelector(this.config.removeBtnSelector);
    if (removeBtn) {
      const handler = () => this.removeItem(item);
      this.handlers.set(removeBtn, handler);
      removeBtn.addEventListener("click", handler);
    }
    if (this.config.showInRowAddButton) {
      const addBtn = item.querySelector(this.config.addBtnSelector);
      if (addBtn) {
        const handler = () => this.addItem();
        this.handlers.set(addBtn, handler);
        addBtn.addEventListener("click", handler);
      }
    }
  }

  /** 删除一行并刷新空状态/按钮 */
  removeItem(item) {
    item.remove();
    this.ensureContainer();
    this.updateEmptyState();
    this.updateAddButtons();
  }

  /** 子类必须实现：构造单行 DOM */
  createRow(_data) {
    throw new Error("DynamicRowManager.createRow must be implemented by subclass");
  }

  /** 子类实现：收集全部行数据，默认返回空数组 */
  collectData() {
    return [];
  }
}
