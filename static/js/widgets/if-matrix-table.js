import { el, clearChildren, tr } from '../core/dom-safe.js';
import './if-citation-chip.js';

const VALID_MODES = new Set(['claim_chart', 'engineering_spec', 'paper_baseline']);

export class IfMatrixTable extends HTMLElement {
  static get observedAttributes() {
    return ['matrix-mode', 'columns-data', 'rows-data'];
  }

  constructor() {
    super();
    this._columns = [];
    this._rows = [];
  }

  connectedCallback() {
    this._syncFromAttributes();
    this._render();
  }

  attributeChangedCallback(name) {
    if (name === 'columns-data' || name === 'rows-data') {
      this._syncFromAttributes();
    }
    if (this.isConnected) {
      this._render();
    }
  }

  get matrixMode() {
    const raw = this.getAttribute('matrix-mode') || 'engineering_spec';
    return VALID_MODES.has(raw) ? raw : 'engineering_spec';
  }

  set matrixMode(val) {
    const safe = VALID_MODES.has(val) ? val : 'engineering_spec';
    this.setAttribute('matrix-mode', safe);
  }

  get columnsData() {
    return [...this._columns];
  }

  set columnsData(cols) {
    this._columns = Array.isArray(cols) ? cols : [];
    if (this.isConnected) this._render();
  }

  get rowsData() {
    return [...this._rows];
  }

  set rowsData(rows) {
    this._rows = Array.isArray(rows) ? rows : [];
    if (this.isConnected) this._render();
  }

  _syncFromAttributes() {
    const colAttr = this.getAttribute('columns-data');
    if (colAttr) {
      try {
        const parsed = JSON.parse(colAttr);
        if (Array.isArray(parsed)) this._columns = parsed;
      } catch {
        // Ignore malformed JSON attribute safely
      }
    }
    const rowAttr = this.getAttribute('rows-data');
    if (rowAttr) {
      try {
        const parsed = JSON.parse(rowAttr);
        if (Array.isArray(parsed)) this._rows = parsed;
      } catch {
        // Ignore malformed JSON attribute safely
      }
    }
  }

  _modeLabel(mode) {
    if (mode === 'claim_chart') return tr('widget.matrix.modeClaimChart', null, 'Claim Chart');
    if (mode === 'paper_baseline') return tr('widget.matrix.modePaperBaseline', null, 'Paper Baseline');
    return tr('widget.matrix.modeEngineeringSpec', null, 'Engineering Spec');
  }

  _emitExport(format) {
    this.dispatchEvent(new CustomEvent('if-export-request', {
      bubbles: true,
      composed: true,
      detail: {
        format,
        mode: this.matrixMode,
      },
    }));
  }

  _emitCellAnchorClick(rowId, colKey, anchor, value) {
    this.dispatchEvent(new CustomEvent('if-cell-anchor-click', {
      bubbles: true,
      composed: true,
      detail: {
        rowId,
        colKey,
        docId: anchor && anchor.docId ? String(anchor.docId) : '',
        anchorTag: anchor && anchor.anchorTag ? String(anchor.anchorTag) : '',
        verdict: anchor && anchor.verdict ? String(anchor.verdict) : 'unverified',
        value: value == null ? '' : String(value),
      },
    }));
  }

  _renderCellContent(rowId, colKey, cell) {
    const wrap = el('div', { className: 'if-matrix-cell' });
    const isObj = cell !== null && typeof cell === 'object' && !Array.isArray(cell);
    const textVal = isObj ? (cell.text ?? cell.value ?? '') : (cell ?? '');
    const badge = isObj && cell.badge ? String(cell.badge) : '';
    const anchors = isObj && Array.isArray(cell.anchors) ? cell.anchors : [];

    if (badge) {
      const badgeTone = isObj && cell.tone ? String(cell.tone) : 'neutral';
      wrap.appendChild(el('span', {
        className: `if-matrix-badge if-matrix-badge--${badgeTone}`,
        text: badge,
      }));
    }

    if (textVal !== '' && textVal !== null && textVal !== undefined) {
      wrap.appendChild(el('span', {
        className: 'if-matrix-text',
        text: String(textVal),
      }));
    }

    if (anchors.length) {
      const chipList = el('div', { className: 'if-matrix-chips' });
      for (const anchor of anchors) {
        const chip = el('if-citation-chip', {
          'doc-id': anchor && anchor.docId != null ? String(anchor.docId) : '',
          'anchor-tag': anchor && anchor.anchorTag != null ? String(anchor.anchorTag) : '',
          verdict: anchor && anchor.verdict ? String(anchor.verdict) : 'unverified',
          summary: anchor && anchor.summary ? String(anchor.summary) : '',
        });
        chip.addEventListener('if-open-source', () => {
          this._emitCellAnchorClick(rowId, colKey, anchor, textVal);
        });
        chipList.appendChild(chip);
      }
      wrap.appendChild(chipList);
    } else if (isObj && (cell.docId || cell.anchorTag)) {
      wrap.addEventListener('click', () => {
        this._emitCellAnchorClick(rowId, colKey, cell, textVal);
      });
    }

    return wrap;
  }

  _render() {
    clearChildren(this);

    const mode = this.matrixMode;
    const headerBar = el('div', { className: 'if-matrix-bar' }, [
      el('span', { className: 'if-matrix-mode', text: this._modeLabel(mode) }),
      el('div', { className: 'if-matrix-export-actions' }, [
        el('button', {
          type: 'button',
          className: 'if-mm-btn if-mm-btn--ghost',
          text: tr('widget.matrix.exportExcel', null, 'Export Excel'),
          onClick: () => this._emitExport('xlsx'),
        }),
        el('button', {
          type: 'button',
          className: 'if-mm-btn if-mm-btn--ghost',
          text: tr('widget.matrix.exportPdf', null, 'Export PDF'),
          onClick: () => this._emitExport('pdf'),
        }),
      ]),
    ]);

    if (!this._columns.length) {
      this.appendChild(el('div', { className: 'if-matrix-wrap' }, [
        headerBar,
        el('div', {
          className: 'if-matrix-empty',
          text: tr('widget.matrix.empty', null, 'No comparison matrix data yet'),
        }),
      ]));
      return;
    }

    const theadRow = el('tr');
    for (const col of this._columns) {
      const label = typeof col === 'string' ? col : (col.label || col.title || col.key || '');
      theadRow.appendChild(el('th', { className: 'if-matrix-th', text: String(label) }));
    }

    const tbody = el('tbody');
    for (let rIdx = 0; rIdx < this._rows.length; rIdx++) {
      const row = this._rows[rIdx] || {};
      const rowId = row.id != null ? String(row.id) : `row-${rIdx}`;
      const trNode = el('tr', {
        className: row.crossEmbodimentWarning ? 'if-matrix-row is-warning' : 'if-matrix-row',
      });
      for (const col of this._columns) {
        const colKey = typeof col === 'string' ? col : (col.key || col.id || col.label || '');
        const cellVal = row.cells && Object.prototype.hasOwnProperty.call(row.cells, colKey)
          ? row.cells[colKey]
          : row[colKey];
        trNode.appendChild(el('td', { className: 'if-matrix-td' }, [
          this._renderCellContent(rowId, colKey, cellVal),
        ]));
      }
      tbody.appendChild(trNode);
    }

    const table = el('table', { className: 'if-matrix-table' }, [
      el('thead', null, [theadRow]),
      tbody,
    ]);

    this.appendChild(el('div', { className: 'if-matrix-wrap' }, [
      headerBar,
      el('div', { className: 'if-matrix-scroll' }, [table]),
    ]));
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('if-matrix-table')) {
  customElements.define('if-matrix-table', IfMatrixTable);
}
