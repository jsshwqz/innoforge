import { el, clearChildren, tr } from '../core/dom-safe.js';
import { apiGet } from '../core/api-client.js';

export class IfVaultPicker extends HTMLElement {
  static get observedAttributes() {
    return ['filter-roles', 'multi-select', 'open'];
  }

  constructor() {
    super();
    this._items = [];
    this._selectedIds = new Set();
    this._query = '';
    this._loading = false;
  }

  connectedCallback() {
    this._render();
  }

  attributeChangedCallback() {
    if (this.isConnected) {
      this._render();
    }
  }

  get filterRoles() {
    const raw = this.getAttribute('filter-roles') || '';
    return raw.split(',').map((s) => s.trim()).filter(Boolean);
  }

  get multiSelect() {
    if (!this.hasAttribute('multi-select')) return false;
    const val = this.getAttribute('multi-select');
    return val !== 'false' && val !== '0';
  }

  get isOpen() {
    if (!this.hasAttribute('open')) return false;
    const val = this.getAttribute('open');
    return val !== 'false' && val !== '0';
  }

  get items() {
    return [...this._items];
  }

  set items(list) {
    this._items = Array.isArray(list)
      ? list.map((it, idx) => ({
          id: it && it.id != null ? String(it.id) : `vault-${idx}`,
          title: it && it.title != null ? String(it.title) : String(it?.id || ''),
          role: it && it.role ? String(it.role) : 'reference',
          kind: it && it.kind ? String(it.kind) : (it && it.isExperiment ? 'experiment' : 'doc'),
          docType: it && it.docType ? String(it.docType) : '',
          filingStatus: it && it.filingStatus ? String(it.filingStatus) : '',
          isExperiment: Boolean(it && (it.isExperiment || it.kind === 'experiment')),
        }))
      : [];
    if (this.isConnected) this._render();
  }

  get selectedIds() {
    return [...this._selectedIds];
  }

  set selectedIds(ids) {
    this._selectedIds = new Set(Array.isArray(ids) ? ids.map((id) => String(id)) : []);
    if (this.isConnected) this._render();
  }

  show() {
    this.setAttribute('open', '');
  }

  hide() {
    this.removeAttribute('open');
  }

  async loadFromApi(endpoint = '/api/vault/docs') {
    this._loading = true;
    if (this.isConnected) this._render();
    try {
      const data = await apiGet(endpoint);
      const list = Array.isArray(data) ? data : (data && Array.isArray(data.items) ? data.items : []);
      this.items = list;
    } finally {
      this._loading = false;
      if (this.isConnected) this._render();
    }
  }

  _filteredItems() {
    const roles = this.filterRoles;
    const q = this._query.trim().toLowerCase();
    return this._items.filter((item) => {
      if (roles.length && !roles.includes(item.role)) return false;
      if (q && !item.title.toLowerCase().includes(q) && !item.id.toLowerCase().includes(q)) {
        return false;
      }
      return true;
    });
  }

  _toggleSelection(itemId) {
    if (this.multiSelect) {
      if (this._selectedIds.has(itemId)) this._selectedIds.delete(itemId);
      else this._selectedIds.add(itemId);
    } else {
      this._selectedIds.clear();
      this._selectedIds.add(itemId);
    }
    this._emitSelectChange();
    this._render();
  }

  _emitSelectChange() {
    const selectedItems = this._items.filter((it) => this._selectedIds.has(it.id));
    const docIds = selectedItems.filter((it) => !it.isExperiment).map((it) => it.id);
    const experimentIds = selectedItems.filter((it) => it.isExperiment).map((it) => it.id);

    this.dispatchEvent(new CustomEvent('if-select-change', {
      bubbles: true,
      composed: true,
      detail: {
        docIds,
        experimentIds,
        selectedItems,
      },
    }));
  }

  _render() {
    clearChildren(this);
    if (!this.isOpen) {
      this.hidden = true;
      return;
    }
    this.hidden = false;

    const filtered = this._filteredItems();

    const listNode = el('div', { className: 'if-vp-list', role: 'listbox' });
    if (this._loading) {
      listNode.appendChild(el('div', {
        className: 'if-vp-empty',
        text: tr('widget.vault.loading', null, 'Loading vault assets…'),
      }));
    } else if (!filtered.length) {
      listNode.appendChild(el('div', {
        className: 'if-vp-empty',
        text: tr('widget.vault.empty', null, 'No matching vault records'),
      }));
    } else {
      for (const item of filtered) {
        const checked = this._selectedIds.has(item.id);
        const input = el('input', {
          type: this.multiSelect ? 'checkbox' : 'radio',
          name: 'if-vault-picker-item',
          checked,
          onChange: () => this._toggleSelection(item.id),
        });
        const metaText = [item.role, item.docType, item.filingStatus].filter(Boolean).join(' · ');
        const row = el('label', { className: checked ? 'if-vp-item is-selected' : 'if-vp-item' }, [
          input,
          el('span', { className: 'if-vp-item-body' }, [
            el('span', { className: 'if-vp-item-title', text: item.title }),
            metaText ? el('span', { className: 'if-vp-item-meta', text: metaText }) : null,
          ]),
        ]);
        listNode.appendChild(row);
      }
    }

    const searchInput = el('input', {
      type: 'search',
      className: 'if-vp-search',
      placeholder: tr('widget.vault.searchPlaceholder', null, 'Filter vault docs or experiments…'),
      value: this._query,
      onInput: (ev) => {
        this._query = ev.target ? ev.target.value : '';
        this._render();
      },
    });
    searchInput.value = this._query;

    const dialog = el('div', { className: 'if-vp-backdrop' }, [
      el('div', { className: 'if-vp-dialog', role: 'dialog', 'aria-modal': 'true' }, [
        el('div', { className: 'if-vp-header' }, [
          el('h3', { className: 'if-vp-title', text: tr('widget.vault.title', null, 'Select Local Vault Assets') }),
          el('button', {
            type: 'button',
            className: 'if-vp-close',
            'aria-label': tr('widget.vault.cancel', null, 'Cancel'),
            text: '×',
            onClick: () => this.hide(),
          }),
        ]),
        el('div', { className: 'if-vp-controls' }, [searchInput]),
        listNode,
        el('div', { className: 'if-vp-footer' }, [
          el('span', {
            className: 'if-vp-count',
            text: tr('widget.vault.selectedCount', { n: this._selectedIds.size }, `Selected: ${this._selectedIds.size}`),
          }),
          el('div', { className: 'if-vp-actions' }, [
            el('button', {
              type: 'button',
              className: 'if-mm-btn if-mm-btn--ghost',
              text: tr('widget.vault.cancel', null, 'Cancel'),
              onClick: () => this.hide(),
            }),
            el('button', {
              type: 'button',
              className: 'if-mm-btn if-mm-btn--primary',
              text: tr('widget.vault.confirm', null, 'Confirm'),
              onClick: () => {
                this._emitSelectChange();
                this.hide();
              },
            }),
          ]),
        ]),
      ]),
    ]);

    this.appendChild(dialog);
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('if-vault-picker')) {
  customElements.define('if-vault-picker', IfVaultPicker);
}
