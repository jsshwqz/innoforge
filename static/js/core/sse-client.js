// Unified SSE client supporting the 7 standard InnoForge envelope events.

export const SSE_EVENTS = Object.freeze([
  'stage_update',
  'token_delta',
  'checkpoint_required',
  'citation_audit',
  'gate_result',
  'completed',
  'error',
]);

function parseEventData(raw) {
  if (typeof raw !== 'string' || raw.length === 0) return {};
  try {
    return JSON.parse(raw);
  } catch {
    return { raw };
  }
}

export function connectSSE(url, handlers = {}, options = {}) {
  const {
    withCredentials = true,
    closeOnComplete = true,
    closeOnError = true,
    closeOnConnectionError = false,
  } = options;

  const source = new EventSource(url, { withCredentials });
  let closed = false;

  const close = () => {
    if (closed) return;
    closed = true;
    source.close();
  };

  for (const eventName of SSE_EVENTS) {
    source.addEventListener(eventName, (event) => {
      const payload = parseEventData(event.data);
      if (typeof handlers[eventName] === 'function') {
        handlers[eventName](payload, event);
      }
      if (typeof handlers.onEvent === 'function') {
        handlers.onEvent(eventName, payload, event);
      }
      if (eventName === 'completed' && closeOnComplete) {
        close();
      } else if (eventName === 'error' && closeOnError) {
        close();
      }
    });
  }

  source.addEventListener('message', (event) => {
    const payload = parseEventData(event.data);
    if (typeof handlers.onMessage === 'function') {
      handlers.onMessage(payload, event);
    }
  });

  source.onerror = (event) => {
    if (typeof handlers.onConnectionError === 'function') {
      handlers.onConnectionError(event);
    }
    if (closeOnConnectionError || source.readyState === 2) {
      close();
    }
  };

  return { source, close };
}
