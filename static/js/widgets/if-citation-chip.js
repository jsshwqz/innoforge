import { el, clearChildren, tr } from '../core/dom-safe.js';

const VALID_VERDICTS = new Set(['verified', 'summary', 'unverified']);

export class IfCitationChip extends HTMLElement {
  static get observedAttributes() {
    return ['doc-id', 'anchor-tag', 'verdict', 'summary'];
  }

  connectedCallback() {
    this._render();
  }

  attributeChangedCallback() {
    if (this.isConnected) {
      this._render();
    }
  }

  get docId() {
    return this.getAttribute('doc-id') || '';
  }

  set docId(val) {
    this.setAttribute('doc-id', val == null ? '' : String(val));
  }

  get anchorTag() {
    return this.getAttribute('anchor-tag') || '';
  }

  set anchorTag(val) {
    this.setAttribute('anchor-tag', val == null ? '' : String(val));
  }

  get verdict() {
    const raw = this.getAttribute('verdict') || 'unverified';
    return VALID_VERDICTS.has(raw) ? raw : 'unverified';
  }

  set verdict(val) {
    const safe = VALID_VERDICTS.has(val) ? val : 'unverified';
    this.setAttribute('verdict', safe);
  }

  get summary() {
    return this.getAttribute('summary') || '';
  }

  set summary(val) {
    if (val === null || val === undefined || val === '') {
      this.removeAttribute('summary');
    } else {
      this.setAttribute('summary', String(val));
    }
  }

  _verdictLabel(verdict) {
    if (verdict === 'verified') return tr('widget.chip.verified', null, 'Verified');
    if (verdict === 'summary') return tr('widget.chip.summary', null, 'Summary');
    return tr('widget.chip.unverified', null, 'Unverified');
  }

  _render() {
    clearChildren(this);

    const docId = this.docId;
    const anchorTag = this.anchorTag || tr('widget.chip.anchorFallback', null, 'Anchor');
    const verdict = this.verdict;
    const summary = this.summary;
    const verdictLabel = this._verdictLabel(verdict);
    const tooltip = summary ? `[${verdictLabel}] ${summary}` : `[${verdictLabel}] ${anchorTag}`;

    const btn = el('button', {
      type: 'button',
      className: `if-chip if-chip--${verdict}`,
      title: tooltip,
      'aria-label': tooltip,
      onClick: (ev) => {
        ev.stopPropagation();
        this.dispatchEvent(new CustomEvent('if-open-source', {
          bubbles: true,
          composed: true,
          detail: {
            docId,
            anchorTag,
            verdict,
            summary,
          },
        }));
      },
    }, [
      el('span', { className: 'if-chip-dot', 'aria-hidden': 'true' }),
      el('span', { className: 'if-chip-tag', text: anchorTag }),
    ]);

    this.appendChild(btn);
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('if-citation-chip')) {
  customElements.define('if-citation-chip', IfCitationChip);
}
