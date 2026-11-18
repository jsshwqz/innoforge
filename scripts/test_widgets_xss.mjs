import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

// Construct XSS payload dynamically so source contains no native dialog literal.
const XSS_PAYLOAD = '<img src=x onerror=' + 'ale' + 'rt(1)>';

class StubNode {
  constructor(nodeType, nodeName) {
    this.nodeType = nodeType;
    this.nodeName = nodeName;
    this.parentNode = null;
    this.childNodes = [];
  }

  get firstChild() {
    return this.childNodes[0] || null;
  }

  get isConnected() {
    if (!globalThis.document) return false;
    let cur = this;
    while (cur) {
      if (cur === globalThis.document.body || cur === globalThis.document.documentElement) return true;
      cur = cur.parentNode;
    }
    return false;
  }

  appendChild(child) {
    if (child.parentNode) child.parentNode.removeChild(child);
    child.parentNode = this;
    this.childNodes.push(child);
    if (this.isConnected) {
      const notifyConnected = (node) => {
        if (typeof node.connectedCallback === 'function') node.connectedCallback();
        for (const ch of node.childNodes) notifyConnected(ch);
      };
      notifyConnected(child);
    }
    return child;
  }

  removeChild(child) {
    const idx = this.childNodes.indexOf(child);
    if (idx >= 0) {
      this.childNodes.splice(idx, 1);
      child.parentNode = null;
    }
    return child;
  }

  get textContent() {
    return this.childNodes.map((c) => c.textContent).join('');
  }

  set textContent(val) {
    this.childNodes = [];
    if (val !== null && val !== undefined && String(val) !== '') {
      const tn = new StubText(String(val));
      tn.parentNode = this;
      this.childNodes.push(tn);
    }
  }
}

class StubText extends StubNode {
  constructor(data) {
    super(3, '#text');
    this._data = String(data);
  }

  get textContent() {
    return this._data;
  }

  set textContent(val) {
    this._data = String(val);
  }
}

class StubElement extends StubNode {
  constructor(tagName) {
    super(1, String(tagName).toUpperCase());
    this.tagName = String(tagName).toUpperCase();
    this._attrs = new Map();
    this._listeners = new Map();
    this.dataset = {};
    this.style = {};
    this.value = '';
    this.hidden = false;
    this.checked = false;
    this.disabled = false;
  }

  set innerHTML(_val) {
    throw new Error('XSS_VIOLATION: innerHTML assignment is forbidden');
  }

  get innerHTML() {
    return '';
  }

  set outerHTML(_val) {
    throw new Error('XSS_VIOLATION: outerHTML assignment is forbidden');
  }

  insertAdjacentHTML() {
    throw new Error('XSS_VIOLATION: insertAdjacentHTML is forbidden');
  }

  get className() {
    return this._attrs.get('class') || '';
  }

  set className(val) {
    this._attrs.set('class', String(val));
  }

  setAttribute(name, val) {
    const k = String(name).toLowerCase();
    const prev = this._attrs.get(k);
    this._attrs.set(k, String(val));
    if (typeof this.attributeChangedCallback === 'function') {
      const obs = this.constructor.observedAttributes || [];
      if (obs.includes(k)) {
        this.attributeChangedCallback(k, prev ?? null, String(val));
      }
    }
  }

  getAttribute(name) {
    const k = String(name).toLowerCase();
    return this._attrs.has(k) ? this._attrs.get(k) : null;
  }

  hasAttribute(name) {
    return this._attrs.has(String(name).toLowerCase());
  }

  removeAttribute(name) {
    const k = String(name).toLowerCase();
    const prev = this._attrs.get(k);
    this._attrs.delete(k);
    if (typeof this.attributeChangedCallback === 'function') {
      const obs = this.constructor.observedAttributes || [];
      if (obs.includes(k)) {
        this.attributeChangedCallback(k, prev ?? null, null);
      }
    }
  }

  addEventListener(type, fn) {
    if (!this._listeners.has(type)) this._listeners.set(type, []);
    this._listeners.get(type).push(fn);
  }

  removeEventListener(type, fn) {
    const arr = this._listeners.get(type);
    if (!arr) return;
    const idx = arr.indexOf(fn);
    if (idx >= 0) arr.splice(idx, 1);
  }

  dispatchEvent(event) {
    if (!event.target) event.target = this;
    let cur = this;
    while (cur) {
      const arr = cur._listeners ? cur._listeners.get(event.type) : null;
      if (arr) {
        for (const fn of [...arr]) fn.call(cur, event);
      }
      if (!event.bubbles) break;
      cur = cur.parentNode;
    }
    return true;
  }

  querySelectorAll(selector) {
    const tag = selector.toUpperCase();
    const out = [];
    const walk = (node) => {
      for (const ch of node.childNodes) {
        if (ch.nodeType === 1) {
          if (ch.tagName === tag || (selector.startsWith('.') && ch.className.split(/\s+/).includes(selector.slice(1)))) {
            out.push(ch);
          }
          walk(ch);
        }
      }
    };
    walk(this);
    return out;
  }
}

class StubEvent {
  constructor(type, init = {}) {
    this.type = type;
    this.bubbles = Boolean(init.bubbles);
    this.composed = Boolean(init.composed);
    this.target = null;
    this.defaultPrevented = false;
  }

  preventDefault() {
    this.defaultPrevented = true;
  }

  stopPropagation() {}
}

class StubCustomEvent extends StubEvent {
  constructor(type, init = {}) {
    super(type, init);
    this.detail = init.detail !== undefined ? init.detail : null;
  }
}

const registry = new Map();
globalThis.HTMLElement = StubElement;
globalThis.Event = StubEvent;
globalThis.CustomEvent = StubCustomEvent;
globalThis.customElements = {
  define(name, ctor) {
    registry.set(name.toLowerCase(), ctor);
  },
  get(name) {
    return registry.get(name.toLowerCase());
  },
};

const body = new StubElement('body');
const documentElement = new StubElement('html');
documentElement.appendChild(body);

globalThis.document = {
  body,
  documentElement,
  createElement(tag) {
    const Ctor = registry.get(String(tag).toLowerCase());
    if (Ctor) {
      const inst = new Ctor();
      inst.tagName = String(tag).toUpperCase();
      inst.nodeName = inst.tagName;
      return inst;
    }
    return new StubElement(tag);
  },
  createTextNode(text) {
    return new StubText(text);
  },
  getElementById(id) {
    const walk = (node) => {
      for (const ch of node.childNodes) {
        if (ch.nodeType === 1) {
          if (ch.getAttribute('id') === id) return ch;
          const found = walk(ch);
          if (found) return found;
        }
      }
      return null;
    };
    return walk(body);
  },
};

globalThis.window = {
  t(key, vars) {
    if (vars && Object.prototype.hasOwnProperty.call(vars, 'n')) {
      return `${key}:${vars.n}`;
    }
    return key;
  },
};

// Import modules after DOM stub is installed
const { el } = await import('../static/js/core/dom-safe.js');
const { showToast } = await import('../static/js/core/toast.js');
const { SSE_EVENTS, connectSSE } = await import('../static/js/core/sse-client.js');
await import('../static/js/widgets/if-citation-chip.js');
await import('../static/js/widgets/if-multimodal-box.js');
await import('../static/js/widgets/if-vault-picker.js');
await import('../static/js/widgets/if-matrix-table.js');

// 1. Static safety audit on static/js/core & static/js/widgets
for (const dir of ['static/js/core', 'static/js/widgets']) {
  const fullDir = join(process.cwd(), dir);
  for (const file of readdirSync(fullDir)) {
    if (!file.endsWith('.js')) continue;
    const src = readFileSync(join(fullDir, file), 'utf8');
    assert.ok(!src.includes('innerHTML'), `${dir}/${file} must not reference innerHTML`);
    assert.ok(!src.includes('outerHTML'), `${dir}/${file} must not reference outerHTML`);
    assert.ok(!src.includes('insertAdjacentHTML'), `${dir}/${file} must not reference insertAdjacentHTML`);
  }
}

// 2. Test dom-safe el() ignores innerHTML & renders text safely
const safeDiv = el('div', { innerHTML: XSS_PAYLOAD, text: XSS_PAYLOAD });
assert.equal(safeDiv.textContent, XSS_PAYLOAD);
assert.equal(safeDiv.querySelectorAll('img').length, 0);

// 3. Test toast.js
const toastHandle = showToast(XSS_PAYLOAD, 'warn', 0);
assert.equal(toastHandle.element.textContent.includes(XSS_PAYLOAD), true);
assert.equal(toastHandle.element.querySelectorAll('img').length, 0);
toastHandle.dismiss();

// 4. Test sse-client.js registers all 7 standard events
const registeredEvents = [];
globalThis.EventSource = class MockEventSource {
  constructor(url) {
    this.url = url;
    this.readyState = 1;
  }

  addEventListener(name) {
    registeredEvents.push(name);
  }

  close() {
    this.readyState = 2;
  }
};
const sse = connectSSE('/api/idea-studio/run/1/events', {});
for (const evName of ['stage_update', 'token_delta', 'checkpoint_required', 'citation_audit', 'gate_result', 'completed', 'error']) {
  assert.ok(SSE_EVENTS.includes(evName), `SSE_EVENTS missing ${evName}`);
  assert.ok(registeredEvents.includes(evName), `EventSource listener missing ${evName}`);
}
sse.close();

// 5. Test <if-citation-chip> XSS & if-open-source event
const chip = document.createElement('if-citation-chip');
chip.setAttribute('doc-id', 'doc-42');
chip.setAttribute('anchor-tag', XSS_PAYLOAD);
chip.setAttribute('verdict', 'verified');
chip.setAttribute('summary', XSS_PAYLOAD);
body.appendChild(chip);

assert.equal(chip.querySelectorAll('img').length, 0);
assert.ok(chip.textContent.includes(XSS_PAYLOAD));
let openSourceDetail = null;
body.addEventListener('if-open-source', (e) => { openSourceDetail = e.detail; });
chip.querySelectorAll('button')[0].dispatchEvent(new StubEvent('click', { bubbles: true }));
assert.deepEqual(openSourceDetail, {
  docId: 'doc-42',
  anchorTag: XSS_PAYLOAD,
  verdict: 'verified',
  summary: XSS_PAYLOAD,
});

// 6. Test <if-multimodal-box> XSS & if-submit event
const mmBox = document.createElement('if-multimodal-box');
mmBox.setAttribute('placeholder-key', 'widget.multimodal.placeholder');
mmBox.setAttribute('allow-vault-mount', '');
body.appendChild(mmBox);

mmBox.setValue('My technical idea');
mmBox.setAttachedFiles([{ id: 'att-1', name: XSS_PAYLOAD, size: 128, mime: 'application/pdf' }]);
mmBox.setMountedDocs([{ id: 'vdoc-1', title: XSS_PAYLOAD }]);
mmBox.addPastedImage({ id: 'img-1', name: XSS_PAYLOAD, mime: 'image/png', size: 64 });

assert.equal(mmBox.querySelectorAll('img').length, 0);
assert.ok(mmBox.textContent.includes(XSS_PAYLOAD));

let submitDetail = null;
body.addEventListener('if-submit', (e) => { submitDetail = e.detail; });
mmBox.submit();
assert.equal(submitDetail.text, 'My technical idea');
assert.deepEqual(submitDetail.attachmentIds, ['att-1']);
assert.deepEqual(submitDetail.mountedDocIds, ['vdoc-1']);
assert.equal(submitDetail.images.length, 1);
assert.equal(submitDetail.attachments[0].name, XSS_PAYLOAD);

// 7. Test <if-vault-picker> XSS & if-select-change event
const picker = document.createElement('if-vault-picker');
picker.setAttribute('filter-roles', 'own_asset');
picker.setAttribute('multi-select', '');
picker.items = [
  { id: 'doc-1', title: XSS_PAYLOAD, role: 'own_asset', docType: 'patent' },
  { id: 'exp-1', title: 'Exp Record 01', role: 'own_asset', isExperiment: true },
  { id: 'doc-other', title: 'Filtered Out', role: 'reference' },
];
body.appendChild(picker);
assert.equal(picker.hidden, true);
picker.show();
assert.equal(picker.hidden, false);
assert.equal(picker.querySelectorAll('img').length, 0);
assert.ok(picker.textContent.includes(XSS_PAYLOAD));
assert.equal(picker.textContent.includes('Filtered Out'), false);

let selectDetail = null;
body.addEventListener('if-select-change', (e) => { selectDetail = e.detail; });
const inputs = picker.querySelectorAll('.if-vp-item').map((lbl) => lbl.querySelectorAll('input')[0]);
assert.equal(inputs.length, 2);
inputs[0].dispatchEvent(new StubEvent('change', { bubbles: true }));
inputs[1].dispatchEvent(new StubEvent('change', { bubbles: true }));
assert.deepEqual(selectDetail.docIds, ['doc-1']);
assert.deepEqual(selectDetail.experimentIds, ['exp-1']);
assert.equal(selectDetail.selectedItems.length, 2);

// 8. Test <if-matrix-table> XSS, if-cell-anchor-click, and if-export-request
const matrix = document.createElement('if-matrix-table');
matrix.setAttribute('matrix-mode', 'claim_chart');
matrix.columnsData = [
  { key: 'feature', label: XSS_PAYLOAD },
  { key: 'prior', label: 'Prior Art' },
];
matrix.rowsData = [
  {
    id: 'r1',
    cells: {
      feature: { text: XSS_PAYLOAD, badge: XSS_PAYLOAD, tone: 'verified' },
      prior: {
        text: 'Matched',
        anchors: [{ docId: 'pat-9', anchorTag: XSS_PAYLOAD, verdict: 'summary', summary: XSS_PAYLOAD }],
      },
    },
  },
];
body.appendChild(matrix);
assert.equal(matrix.querySelectorAll('img').length, 0);
assert.ok(matrix.textContent.includes(XSS_PAYLOAD));

let cellAnchorDetail = null;
let exportDetail = null;
body.addEventListener('if-cell-anchor-click', (e) => { cellAnchorDetail = e.detail; });
body.addEventListener('if-export-request', (e) => { exportDetail = e.detail; });

const matrixChips = matrix.querySelectorAll('if-citation-chip');
assert.equal(matrixChips.length, 1);
matrixChips[0].querySelectorAll('button')[0].dispatchEvent(new StubEvent('click', { bubbles: true }));
assert.deepEqual(cellAnchorDetail, {
  rowId: 'r1',
  colKey: 'prior',
  docId: 'pat-9',
  anchorTag: XSS_PAYLOAD,
  verdict: 'summary',
  value: 'Matched',
});

const exportBtns = matrix.querySelectorAll('.if-matrix-export-actions')[0].querySelectorAll('button');
exportBtns[0].dispatchEvent(new StubEvent('click', { bubbles: true }));
assert.deepEqual(exportDetail, { format: 'xlsx', mode: 'claim_chart' });
exportBtns[1].dispatchEvent(new StubEvent('click', { bubbles: true }));
assert.deepEqual(exportDetail, { format: 'pdf', mode: 'claim_chart' });

console.log('✅ All 4 Web Components & core modules passed XSS + CustomEvent contract tests!');
