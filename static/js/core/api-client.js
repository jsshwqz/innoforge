// Unified HTTP client with same-origin credentials, timeout control, and structured ApiError.

export class ApiError extends Error {
  constructor(message, { status = 0, code = null, details = null } = {}) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

function isPlainBody(body) {
  if (body === null || body === undefined) return false;
  if (typeof body === 'string') return false;
  if (typeof FormData !== 'undefined' && body instanceof FormData) return false;
  if (typeof Blob !== 'undefined' && body instanceof Blob) return false;
  if (typeof URLSearchParams !== 'undefined' && body instanceof URLSearchParams) return false;
  return typeof body === 'object';
}

export async function apiRequest(url, options = {}) {
  const {
    method = 'GET',
    body = undefined,
    headers = {},
    timeoutMs = 60000,
    signal = undefined,
    credentials = 'same-origin',
  } = options;

  const controller = new AbortController();
  let timerId = null;

  const onExternalAbort = () => controller.abort();
  if (signal) {
    if (signal.aborted) {
      controller.abort();
    } else {
      signal.addEventListener('abort', onExternalAbort, { once: true });
    }
  }

  if (timeoutMs > 0) {
    timerId = setTimeout(() => controller.abort(), timeoutMs);
  }

  const reqHeaders = new Headers(headers);
  let reqBody = body;
  if (isPlainBody(body)) {
    if (!reqHeaders.has('Content-Type')) {
      reqHeaders.set('Content-Type', 'application/json');
    }
    reqBody = JSON.stringify(body);
  }

  try {
    const response = await fetch(url, {
      method,
      headers: reqHeaders,
      body: reqBody,
      credentials,
      signal: controller.signal,
    });

    const contentType = response.headers.get('content-type') || '';
    const isJson = contentType.includes('application/json');
    const payload = isJson
      ? await response.json().catch(() => null)
      : await response.text().catch(() => '');

    if (!response.ok) {
      const msg = (payload && typeof payload === 'object' && (payload.error || payload.message))
        || (typeof payload === 'string' && payload)
        || `HTTP ${response.status}`;
      const code = payload && typeof payload === 'object' ? (payload.code || null) : null;
      throw new ApiError(String(msg), { status: response.status, code, details: payload });
    }

    return payload;
  } catch (err) {
    if (err instanceof ApiError) throw err;
    if (err && err.name === 'AbortError') {
      throw new ApiError('Request aborted or timed out', { status: 0, code: 'TIMEOUT' });
    }
    throw new ApiError(err && err.message ? err.message : 'Network error', { status: 0, code: 'NETWORK' });
  } finally {
    if (timerId !== null) clearTimeout(timerId);
    if (signal) signal.removeEventListener('abort', onExternalAbort);
  }
}

export function apiGet(url, options = {}) {
  return apiRequest(url, { ...options, method: 'GET' });
}

export function apiPost(url, body, options = {}) {
  return apiRequest(url, { ...options, method: 'POST', body });
}

export function apiPatch(url, body, options = {}) {
  return apiRequest(url, { ...options, method: 'PATCH', body });
}

export function apiDelete(url, options = {}) {
  return apiRequest(url, { ...options, method: 'DELETE' });
}
