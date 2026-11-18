// Zero-build safe DOM helpers: strictly uses textContent & setAttribute.

const FORBIDDEN_ATTRS = new Set(['inner' + 'HTML', 'outer' + 'HTML', 'insertAdjacent' + 'HTML']);
const BOOLEAN_ATTRS = new Set([
  'disabled',
  'checked',
  'selected',
  'readOnly',
  'readonly',
  'required',
  'multiple',
  'hidden',
]);

function appendChildSafe(parent, child) {
  if (child === null || child === undefined || child === false) return;
  if (Array.isArray(child)) {
    for (const item of child) appendChildSafe(parent, item);
    return;
  }
  if (typeof child === 'object' && typeof child.nodeType === 'number') {
    parent.appendChild(child);
    return;
  }
  parent.appendChild(document.createTextNode(String(child)));
}

export function el(tag, attrs = null, children = null) {
  const node = document.createElement(tag);
  if (attrs && typeof attrs === 'object') {
    for (const [key, val] of Object.entries(attrs)) {
      if (val === null || val === undefined || FORBIDDEN_ATTRS.has(key)) continue;
      if (key === 'className' || key === 'class') {
        node.className = String(val);
      } else if (key === 'text') {
        node.textContent = String(val);
      } else if (key === 'dataset' && typeof val === 'object') {
        for (const [dk, dv] of Object.entries(val)) {
          if (dv !== null && dv !== undefined) {
            node.dataset[dk] = String(dv);
          }
        }
      } else if (key === 'style') {
        if (typeof val === 'string') {
          node.setAttribute('style', val);
        } else if (typeof val === 'object' && node.style) {
          Object.assign(node.style, val);
        }
      } else if (key.startsWith('on') && typeof val === 'function') {
        node.addEventListener(key.slice(2).toLowerCase(), val);
      } else if (BOOLEAN_ATTRS.has(key)) {
        const boolVal = Boolean(val);
        node[key] = boolVal;
        if (boolVal) {
          node.setAttribute(key.toLowerCase(), '');
        } else {
          node.removeAttribute(key.toLowerCase());
        }
      } else {
        node.setAttribute(key, String(val));
      }
    }
  }
  if (children !== null && children !== undefined) {
    appendChildSafe(node, children);
  }
  return node;
}

export function clearChildren(node) {
  if (!node) return;
  while (node.firstChild) {
    node.removeChild(node.firstChild);
  }
}

export function setText(node, text) {
  if (!node) return;
  node.textContent = text === null || text === undefined ? '' : String(text);
}

export function tr(key, vars = null, fallback = null) {
  if (typeof window !== 'undefined' && typeof window.t === 'function') {
    const translated = window.t(key, vars);
    if (translated && translated !== key) return translated;
  }
  return fallback !== null && fallback !== undefined ? fallback : key;
}
