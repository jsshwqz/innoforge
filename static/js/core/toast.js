import { el, tr } from './dom-safe.js';

const VALID_TYPES = new Set(['info', 'success', 'warn', 'error']);
const CONTAINER_ID = 'if-toast-container';

function getOrCreateContainer() {
  let container = typeof document.getElementById === 'function'
    ? document.getElementById(CONTAINER_ID)
    : null;
  if (!container && document.body) {
    container = el('div', {
      id: CONTAINER_ID,
      className: 'if-toast-container',
      'aria-live': 'polite',
      role: 'region',
    });
    document.body.appendChild(container);
  }
  return container;
}

export function dismissToast(toastNode) {
  if (toastNode && toastNode.parentNode) {
    toastNode.parentNode.removeChild(toastNode);
  }
}

export function showToast(message, type = 'info', durationMs = 3500) {
  const safeType = VALID_TYPES.has(type) ? type : 'info';
  const container = getOrCreateContainer();

  const closeBtn = el('button', {
    type: 'button',
    className: 'if-toast-close',
    'aria-label': tr('widget.toast.close', null, 'Close'),
    text: '×',
  });

  const toastNode = el('div', {
    className: `if-toast if-toast--${safeType}`,
    role: safeType === 'error' ? 'alert' : 'status',
  }, [
    el('span', { className: 'if-toast-msg', text: message === null || message === undefined ? '' : String(message) }),
    closeBtn,
  ]);

  let timerId = null;
  const remove = () => {
    if (timerId !== null) {
      clearTimeout(timerId);
      timerId = null;
    }
    dismissToast(toastNode);
  };

  closeBtn.addEventListener('click', remove);

  if (container) {
    container.appendChild(toastNode);
  }
  if (durationMs > 0) {
    timerId = setTimeout(remove, durationMs);
  }

  return { element: toastNode, dismiss: remove };
}
