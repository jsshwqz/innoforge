import { el, clearChildren, setText, tr } from '../core/dom-safe.js';
import { apiGet, apiPost } from '../core/api-client.js';
import { showToast } from '../core/toast.js';
import { IfCitationChip } from '../widgets/if-citation-chip.js';

if (IfCitationChip && typeof customElements !== 'undefined' && !customElements.get('if-citation-chip')) {
  customElements.define('if-citation-chip', IfCitationChip);
}

const KIND_LABELS = {
  patent: '专利 / Patent',
  journal_paper: '期刊论文 / Journal',
  conf_paper: '会议论文 / Conf',
  preprint: '预印本 / Preprint',
  code_repo: '开源工程 / Code',
  standard: '行业标准 / Standard',
  internal_scheme: '内部方案 / Scheme',
};

let lastOmniHits = [];
let activeDocPayload = null;
let activeNumeralCode = null;

function copyTextWithToast(text, label) {
  if (!text) {
    showToast(tr('omni.copyEmpty', null, '无可复制内容'), 'warn');
    return;
  }
  if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
    navigator.clipboard.writeText(text).then(
      () => showToast(`${label || '内容'}已复制到剪贴板`, 'success'),
      () => showToast('复制失败，请手动选择复制', 'error'),
    );
  } else {
    showToast('当前环境不支持剪贴板 API', 'warn');
  }
}

function getSelectedRealms() {
  const boxes = document.querySelectorAll('input[name="omni-realm"]:checked');
  return Array.from(boxes).map((b) => b.value);
}

function buildCitationChip(docId, anchorTag, verdict, summary) {
  const chip = document.createElement('if-citation-chip');
  chip.setAttribute('doc-id', docId || '');
  chip.setAttribute('anchor-tag', anchorTag || '');
  chip.setAttribute('verdict', verdict || 'summary');
  if (summary) chip.setAttribute('summary', summary);
  return chip;
}

function renderChunkWithNumerals(text, numerals, highlightCode) {
  const p = el('div', { className: 'omni-chunk-body' });
  if (!text) {
    p.textContent = '';
    return p;
  }
  if (!highlightCode || !numerals || numerals.length === 0) {
    p.textContent = text;
    return p;
  }
  const escaped = highlightCode.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const re = new RegExp(`(${escaped})`, 'g');
  const parts = String(text).split(re);
  for (const part of parts) {
    if (part === highlightCode) {
      p.appendChild(el('mark', { className: 'omni-numeral-mark', text: part }));
    } else if (part) {
      p.appendChild(document.createTextNode(part));
    }
  }
  return p;
}

function renderReaderPane(container, payload, targetAnchor = '') {
  if (!container) return;
  clearChildren(container);
  activeDocPayload = payload;

  if (!payload || !payload.doc) {
    container.appendChild(
      el('div', {
        className: 'muted',
        text: tr('omni.readerEmpty', null, '点击左侧任意命中结果，在此同屏精读权项树、段落与附图部件词典。'),
      }),
    );
    return;
  }

  const { doc, doc_chunks: chunks = [], figure_numerals: numerals = [] } = payload;
  const title = doc.title_zh || doc.title_en || doc.doc_id;

  const headerActions = el('div', { className: 'omni-reader-actions' }, [
    el('button', {
      type: 'button',
      className: 'btn-secondary btn-sm',
      text: tr('omni.openStandalone', null, '独立精读页 ↗'),
      onClick: () => {
        window.open(`/doc-viewer?id=${encodeURIComponent(doc.doc_id)}`, '_blank');
      },
    }),
  ]);

  if (doc.bibtex_raw) {
    headerActions.appendChild(
      el('button', {
        type: 'button',
        className: 'btn-secondary btn-sm',
        text: tr('omni.copyBibtex', null, '复制 BibTeX'),
        onClick: () => copyTextWithToast(doc.bibtex_raw, 'BibTeX'),
      }),
    );
  }

  const header = el('div', { className: 'omni-reader-header' }, [
    el('div', {}, [
      el('div', { className: 'omni-reader-meta' }, [
        el('span', { className: 'omni-badge', text: KIND_LABELS[doc.doc_kind] || doc.doc_kind }),
        doc.external_id ? el('code', { className: 'omni-extid', text: doc.external_id }) : null,
        doc.doi_verified
          ? buildCitationChip(doc.doc_id, 'DOI Verified', 'verified', doc.external_id)
          : null,
      ]),
      el('h4', { className: 'omni-reader-title', text: title }),
      doc.authors_or_assignees
        ? el('div', { className: 'muted omni-reader-authors', text: `${doc.authors_or_assignees} · ${doc.pub_or_filing_date || ''}` })
        : null,
    ]),
    headerActions,
  ]);

  container.appendChild(header);

  if (doc.abstract_text) {
    container.appendChild(
      el('div', { className: 'omni-reader-abstract' }, [
        el('strong', { text: tr('omni.abstract', null, '摘要：') }),
        el('span', { text: doc.abstract_text }),
      ]),
    );
  }

  const splitGrid = el('div', { className: 'omni-viewer-split' });

  // 左：权项树与说明书/论文章节切片
  const chunksCol = el('div', { className: 'omni-chunks-col' }, [
    el('h5', { text: tr('omni.chunksTitle', null, `结构化权项树 / 章节 (${chunks.length})`) }),
  ]);

  if (chunks.length === 0) {
    chunksCol.appendChild(
      el('div', {
        className: 'muted',
        text: doc.fulltext_raw
          ? doc.fulltext_raw
          : tr('omni.noChunks', null, '暂无结构化切片，以上摘要与元数据已就绪。'),
      }),
    );
  } else {
    for (const chunk of chunks) {
      const isTarget = targetAnchor && chunk.section_tag === targetAnchor;
      const indent = chunk.parent_claim_no ? 'omni-chunk-child' : '';
      const card = el('div', {
        className: `omni-chunk-item ${indent} ${isTarget ? 'is-highlighted' : ''}`,
        dataset: { anchor: chunk.section_tag },
      }, [
        el('div', { className: 'omni-chunk-head' }, [
          buildCitationChip(doc.doc_id, chunk.section_tag, 'verified', chunk.section_kind),
          chunk.parent_claim_no
            ? el('span', { className: 'muted', text: `↳ 依附 Claim:${chunk.parent_claim_no}` })
            : null,
          chunk.embodiment_group
            ? el('span', { className: 'omni-badge-subtle', text: chunk.embodiment_group })
            : null,
        ]),
        renderChunkWithNumerals(chunk.raw_content, numerals, activeNumeralCode),
      ]);
      chunksCol.appendChild(card);
    }
  }

  // 右：附图部件标号词典（悬停高亮联动）
  const numeralsCol = el('div', { className: 'omni-numerals-col' }, [
    el('h5', { text: tr('omni.numeralsTitle', null, `附图部件词典 (${numerals.length})`) }),
  ]);

  if (numerals.length === 0) {
    numeralsCol.appendChild(
      el('div', { className: 'muted', text: tr('omni.noNumerals', null, '暂无附图部件标号记录。') }),
    );
  } else {
    const list = el('div', { className: 'omni-numeral-list' });
    for (const item of numerals) {
      const pill = el('button', {
        type: 'button',
        className: `omni-numeral-pill ${activeNumeralCode === item.numeral_code ? 'is-active' : ''}`,
        title: `${item.figure_label || ''} · ${item.anchor_tag || ''}`,
        onMouseEnter: () => {
          activeNumeralCode = item.numeral_code;
          renderReaderPane(container, activeDocPayload, targetAnchor);
        },
        onMouseLeave: () => {
          activeNumeralCode = null;
          renderReaderPane(container, activeDocPayload, targetAnchor);
        },
        onClick: () => {
          activeNumeralCode = activeNumeralCode === item.numeral_code ? null : item.numeral_code;
          renderReaderPane(container, activeDocPayload, targetAnchor);
        },
      }, [
        el('strong', { text: item.numeral_code }),
        el('span', { text: ` ${item.part_name}` }),
      ]);
      list.appendChild(pill);
    }
    numeralsCol.appendChild(list);
  }

  splitGrid.appendChild(chunksCol);
  splitGrid.appendChild(numeralsCol);
  container.appendChild(splitGrid);

  if (targetAnchor) {
    const targetEl = Array.from(container.querySelectorAll('[data-anchor]')).find(
      (n) => n.dataset.anchor === targetAnchor,
    );
    if (targetEl && typeof targetEl.scrollIntoView === 'function') {
      targetEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
  }
}

async function openDocInReader(docId, fallbackHit = null, targetAnchor = '') {
  const reader = document.getElementById('omni-reader-pane') || document.getElementById('doc-viewer-content');
  if (!reader) return;
  clearChildren(reader);
  reader.appendChild(el('div', { className: 'muted', text: tr('omni.loadingDoc', null, '正在加载文献结构...') }));

  try {
    const data = await apiGet(`/api/omni-search/doc/${encodeURIComponent(docId)}`);
    renderReaderPane(reader, data, targetAnchor);
  } catch (err) {
    if (fallbackHit && fallbackHit.doc) {
      renderReaderPane(reader, {
        doc: fallbackHit.doc,
        doc_chunks: [],
        figure_numerals: [],
      });
    } else {
      clearChildren(reader);
      reader.appendChild(el('div', { className: 'omni-error', text: err.message || '文献加载失败' }));
    }
  }
}

async function markAsOwnAsset(hit) {
  if (!hit || !hit.doc) return;
  try {
    const res = await apiPost('/api/omni-search/query', {
      query: hit.doc.title_zh || hit.doc.title_en || hit.doc.doc_id,
      asset_role: 'own_asset',
      save_doc: hit.doc,
    });
    if (res && res.saved_doc) {
      hit.doc.asset_role = 'own_asset';
      showToast(tr('omni.markedOwnAsset', null, '已打标为「我方已有成果」并存入本地金库'), 'success');
      renderOmniHits(lastOmniHits);
    }
  } catch (err) {
    showToast(`入库打标失败: ${err.message}`, 'error');
  }
}

function renderOmniHits(hits) {
  const listEl = document.getElementById('omni-results-list');
  if (!listEl) return;
  clearChildren(listEl);

  if (!hits || hits.length === 0) {
    listEl.appendChild(el('div', { className: 'muted', text: tr('omni.emptyHits', null, '未命中匹配的四维文献记录。') }));
    return;
  }

  for (const hit of hits) {
    const doc = hit.doc || {};
    const title = doc.title_zh || doc.title_en || doc.external_id || doc.doc_id;
    const isOwn = doc.asset_role === 'own_asset';

    const card = el('article', { className: 'omni-hit-card' }, [
      el('div', { className: 'omni-hit-top' }, [
        el('span', { className: 'omni-badge', text: KIND_LABELS[doc.doc_kind] || doc.doc_kind || hit.realm }),
        el('span', { className: 'omni-provider-tag', text: hit.source_provider || '' }),
        doc.doi_verified
          ? buildCitationChip(doc.doc_id, doc.external_id || 'DOI', 'verified', '真实 DOI 已核验')
          : null,
        isOwn ? el('span', { className: 'omni-own-tag', text: '我方已有成果' }) : null,
      ]),
      el('h4', { className: 'omni-hit-title' }, [
        el('button', {
          type: 'button',
          className: 'omni-title-link',
          text: title,
          onClick: () => openDocInReader(doc.doc_id, hit),
        }),
      ]),
      doc.authors_or_assignees || doc.pub_or_filing_date
        ? el('div', {
            className: 'muted omni-hit-meta',
            text: `${doc.authors_or_assignees || ''} · ${doc.pub_or_filing_date || ''} · ${doc.venue_or_authority || ''}`,
          })
        : null,
      doc.abstract_text
        ? el('p', { className: 'omni-hit-abs', text: doc.abstract_text.slice(0, 220) })
        : null,
      el('details', { className: 'omni-hit-reason' }, [
        el('summary', { text: tr('omni.matchReason', null, '推荐命中理由') }),
        el('div', { className: 'muted', text: hit.match_reason || '关键词与语义相关性匹配' }),
      ]),
      el('div', { className: 'omni-hit-actions' }, [
        el('button', {
          type: 'button',
          className: 'btn-secondary btn-sm',
          text: tr('omni.readSplit', null, '同屏精读'),
          onClick: () => openDocInReader(doc.doc_id, hit),
        }),
        el('button', {
          type: 'button',
          className: isOwn ? 'btn-secondary btn-sm' : 'btn-primary btn-sm',
          disabled: isOwn,
          text: isOwn ? '✓ 已标为我方成果' : '[我方已有成果]',
          onClick: () => markAsOwnAsset(hit),
        }),
        doc.bibtex_raw
          ? el('button', {
              type: 'button',
              className: 'btn-secondary btn-sm',
              text: 'BibTeX',
              onClick: () => copyTextWithToast(doc.bibtex_raw, 'BibTeX'),
            })
          : null,
      ]),
    ]);

    listEl.appendChild(card);
  }
}

function renderDiagnostics(diagnostics, plan) {
  const diagEl = document.getElementById('omni-diagnostics');
  if (!diagEl) return;
  clearChildren(diagEl);

  if (plan) {
    diagEl.appendChild(
      el('div', { className: 'omni-plan-bar' }, [
        el('span', { className: 'muted', text: `检索式 (${plan.used_ai_rewrite ? 'AI 扩写' : '确定性双语'}): ` }),
        el('code', { text: plan.patent_boolean_query || plan.raw_query }),
      ]),
    );
  }

  if (Array.isArray(diagnostics) && diagnostics.length > 0) {
    const row = el('div', { className: 'omni-diag-row' });
    for (const d of diagnostics) {
      row.appendChild(
        el('span', {
          className: `omni-diag-pill omni-diag--${d.status}`,
          text: `${d.source}: ${d.status} (${d.hit_count})`,
        }),
      );
    }
    diagEl.appendChild(row);
  }
}

export async function runOmniSearch() {
  const input = document.getElementById('omni-query-input');
  const query = input ? input.value.trim() : '';
  if (!query) {
    showToast(tr('omni.emptyQuery', null, '请输入检索关键词或技术构想'), 'warn');
    return;
  }

  const realms = getSelectedRealms();
  const roleEl = document.getElementById('omni-role-filter');
  const assetRole = roleEl && roleEl.value ? roleEl.value : null;
  const aiEl = document.getElementById('omni-ai-rewrite');
  const saveEl = document.getElementById('omni-save-vault');
  const statusEl = document.getElementById('omni-status-text');

  setText(statusEl, tr('omni.searching', null, '正在并发检索专利/论文/开源/标准四大圈层...'));

  try {
    const res = await apiPost('/api/omni-search/query', {
      query,
      realms,
      asset_role: assetRole,
      enable_ai_rewrite: Boolean(aiEl && aiEl.checked),
      save_to_vault: Boolean(saveEl && saveEl.checked),
      max_per_source: 10,
    });
    lastOmniHits = Array.isArray(res.hits) ? res.hits : [];
    renderDiagnostics(res.diagnostics, res.plan);
    renderOmniHits(lastOmniHits);
    setText(
      statusEl,
      `共命中 ${lastOmniHits.length} 条去重结果${res.saved_count ? `（已入库 ${res.saved_count} 条）` : ''}`,
    );
  } catch (err) {
    setText(statusEl, `检索失败: ${err.message}`);
    showToast(err.message || '全域检索失败', 'error');
  }
}

export async function runChunkHybridSearch() {
  const input = document.getElementById('omni-query-input');
  const query = input ? input.value.trim() : '';
  if (!query) {
    showToast('请输入段落召回关键词', 'warn');
    return;
  }

  const listEl = document.getElementById('omni-results-list');
  const statusEl = document.getElementById('omni-status-text');
  setText(statusEl, '正在执行 RRF 混合段落召回...');

  try {
    const res = await apiPost('/api/omni-search/chunks', {
      query,
      mode: 'patent_compare',
      top_k: 10,
      char_budget: 8000,
    });
    const bundle = res.bundle || { hits: [] };
    if (!listEl) return;
    clearChildren(listEl);
    for (const h of bundle.hits || []) {
      const chunk = h.chunk || {};
      listEl.appendChild(
        el('div', { className: 'omni-hit-card' }, [
          el('div', { className: 'omni-hit-top' }, [
            buildCitationChip(chunk.doc_id, chunk.section_tag, 'verified', `RRF=${(h.rrf_score || 0).toFixed(4)}`),
            el('span', { className: 'muted', text: `Doc: ${chunk.doc_id}` }),
          ]),
          el('pre', { className: 'omni-chunk-pre', text: h.assembled_context || chunk.raw_content }),
          el('button', {
            type: 'button',
            className: 'btn-secondary btn-sm',
            text: '在精读器中定位原文',
            onClick: () => openDocInReader(chunk.doc_id, null, chunk.section_tag),
          }),
        ]),
      );
    }
    setText(statusEl, `混合段落召回 ${(bundle.hits || []).length} 条切片（${bundle.total_chars || 0} 字符）`);
  } catch (err) {
    setText(statusEl, `段落召回失败: ${err.message}`);
    showToast(err.message || '段落召回失败', 'error');
  }
}

export async function exportOmniResults(format = 'csv') {
  const input = document.getElementById('omni-query-input');
  const query = input ? input.value.trim() : '';
  if (!query && lastOmniHits.length === 0) {
    showToast('请先执行检索后再导出', 'warn');
    return;
  }

  try {
    const resp = await fetch('/api/omni-search/export', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify({
        query,
        format,
        realms: getSelectedRealms(),
        hits: lastOmniHits,
      }),
    });
    if (!resp.ok) {
      throw new Error(`HTTP ${resp.status}`);
    }
    const blob = await resp.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `omni_search.${format}`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    showToast(`已导出 ${format.toUpperCase()} 文件`, 'success');
  } catch (err) {
    showToast(`导出失败: ${err.message}`, 'error');
  }
}

function initSearchPage() {
  const root = document.getElementById('omni-search-root');
  if (!root) return;

  const searchBtn = document.getElementById('omni-search-btn');
  if (searchBtn) searchBtn.addEventListener('click', runOmniSearch);

  const chunksBtn = document.getElementById('omni-chunks-btn');
  if (chunksBtn) chunksBtn.addEventListener('click', runChunkHybridSearch);

  const queryInput = document.getElementById('omni-query-input');
  if (queryInput) {
    queryInput.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') runOmniSearch();
    });
  }

  document.querySelectorAll('[data-omni-export]').forEach((btn) => {
    btn.addEventListener('click', () => exportOmniResults(btn.dataset.omniExport || 'csv'));
  });

  document.addEventListener('if-open-source', (ev) => {
    const detail = ev.detail || {};
    if (detail.docId) {
      openDocInReader(detail.docId, null, detail.anchorTag || '');
    }
  });
}

function initDocViewerPage() {
  const root = document.getElementById('doc-viewer-root');
  if (!root) return;

  const params = new URLSearchParams(window.location.search);
  let docId = params.get('id') || '';
  if (!docId && window.location.pathname.startsWith('/doc/')) {
    docId = decodeURIComponent(window.location.pathname.slice(5));
  }
  const anchor = params.get('anchor') || '';
  const idInput = document.getElementById('doc-viewer-id-input');
  const loadBtn = document.getElementById('doc-viewer-load-btn');

  if (idInput && docId) idInput.value = docId;
  if (loadBtn) {
    loadBtn.addEventListener('click', () => {
      const val = idInput ? idInput.value.trim() : '';
      if (val) openDocInReader(val, null, '');
    });
  }

  if (docId) {
    openDocInReader(docId, null, anchor);
  }
}

if (typeof document !== 'undefined') {
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      initSearchPage();
      initDocViewerPage();
    });
  } else {
    initSearchPage();
    initDocViewerPage();
  }
}
