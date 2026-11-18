import { apiGet, apiPost, apiPatch, apiDelete } from '../core/api-client.js';
import { el, clearChildren, setText } from '../core/dom-safe.js';
import { showToast } from '../core/toast.js';

const ROLE_OPTIONS = [
  { value: 'own_asset', label: '我方已有成果 (own_asset)' },
  { value: 'competitor_wall', label: '竞品壁垒 (competitor_wall)' },
  { value: 'review_cited', label: '审查引证 (review_cited)' },
  { value: 'general_ref', label: '通用参考 (general_ref)' },
];

const FILING_OPTIONS = [
  { value: 'unfiled_secret', label: '未交底保密 (unfiled_secret)' },
  { value: 'patent_filed', label: '已递交专利 (patent_filed)' },
  { value: 'public_disclosed', label: '已公开披露 (public_disclosed)' },
];

const state = {
  providers: [],
  pendingDeleteDocId: null,
};

function makeStatusCard(title, ok, lines) {
  const badgeColor = ok ? '#3fb950' : '#d29922';
  const card = el('div', {
    style: {
      border: '1px solid var(--border)',
      borderRadius: '8px',
      padding: '10px 12px',
      background: 'var(--bg-card)',
    },
  });
  card.appendChild(
    el('div', { style: { display: 'flex', justifyContent: 'space-between', marginBottom: '6px' } }, [
      el('strong', { text: title, style: { fontSize: '13px' } }),
      el('span', {
        text: ok ? '● 正常' : '▲ 待关注',
        style: { color: badgeColor, fontSize: '12px', fontWeight: '600' },
      }),
    ]),
  );
  for (const line of lines) {
    card.appendChild(
      el('div', {
        text: line,
        style: { fontSize: '12px', color: 'var(--text-secondary)', lineHeight: '1.5' },
      }),
    );
  }
  return card;
}

export async function loadEnvironmentHealth() {
  const grid = document.getElementById('v1-health-grid');
  if (!grid) return;
  try {
    const data = await apiGet('/api/settings/health');
    clearChildren(grid);
    const dims = data.dimensions || {};
    const sq = dims.sqlite || {};
    const net = dims.network_security || {};
    const ai = dims.ai_failover_vision || {};
    const ocr = dims.omni_search_and_ocr || {};

    grid.appendChild(makeStatusCard('① SQLite WAL 与 Schema', Boolean(sq.healthy), [
      `模式: ${sq.journal_mode || '-'} · Schema v${sq.schema_version || 0} (目标 v${sq.expected_version || 26})`,
      `文献 ${sq.knowledge_docs_count || 0} 篇 · 实验 ${sq.experiment_records_count || 0} 条 · 特征原子 ${sq.feature_atoms_count || 0} 个`,
    ]));

    grid.appendChild(makeStatusCard('② 网络绑定与同源鉴权', Boolean(net.healthy), [
      `监听: ${net.bind_ip || '127.0.0.1'}:${net.listen_port || 3000}`,
      net.label || '回环安全监听',
    ]));

    grid.appendChild(makeStatusCard('③ 9 服务商容灾与看图', Boolean(ai.healthy), [
      `主服务商: ${ai.active_provider || '-'} · 远端可用: ${ai.remote_usable_count || 0}/9`,
      `看图模型就绪: ${ai.vision_ready_count || 0} 家 (${ai.vision_ready ? '支持扫描件三级回退' : '未配置看图模型'})`,
    ]));

    grid.appendChild(makeStatusCard('④ 四维检索与三级 OCR', Boolean(ocr.healthy), [
      `免 Key 学术源: 4 家就绪 · SerpAPI: ${ocr.serpapi_key_count || 0} · EPO: ${ocr.epo_configured ? '已配置' : '未配置'}`,
      `三级 OCR: 原生文字层 -> Umi-OCR (${ocr.umi_ocr_url || ''}) -> 看图回退`,
    ]));
  } catch (err) {
    showToast(`健康自检失败: ${err.message}`, 'error');
  }
}

export async function loadRouterConfig() {
  const list = document.getElementById('v1-providers-list');
  const visionSelect = document.getElementById('v1-vision-provider');
  const budgetInput = document.getElementById('v1-daily-budget');
  const offlineInput = document.getElementById('v1-privacy-offline');
  const umiInput = document.getElementById('v1-umi-ocr-url');
  if (!list) return;

  try {
    const cfg = await apiGet('/api/settings/config');
    state.providers = Array.isArray(cfg.providers) ? cfg.providers : [];

    if (budgetInput && cfg.daily_budget) budgetInput.value = String(cfg.daily_budget);
    if (offlineInput) offlineInput.checked = Boolean(cfg.privacy_offline_only);
    if (umiInput && cfg.umi_ocr_url !== undefined) umiInput.value = String(cfg.umi_ocr_url);

    if (visionSelect) {
      clearChildren(visionSelect);
      visionSelect.appendChild(el('option', { value: '', text: '自动选择首个可用看图服务商' }));
      for (const p of state.providers) {
        if (p.supports_vision) {
          visionSelect.appendChild(
            el('option', {
              value: p.id,
              text: `${p.display_name} (${p.vision_model || p.default_model})`,
            }),
          );
        }
      }
      visionSelect.value = cfg.vision_provider_id || '';
    }

    clearChildren(list);
    for (const p of state.providers) {
      const keyInput = el('input', {
        type: 'password',
        value: p.api_key_masked || '',
        placeholder: p.id === 'ollama' ? '本地端点无需 API Key' : '输入新 Key 或保持掩码不变',
        autocomplete: 'off',
        dataset: { providerId: p.id },
        style: { flex: '1', minWidth: '0', fontSize: '12px' },
      });
      if (p.id === 'ollama') keyInput.disabled = true;

      const probeResult = el('span', {
        text: p.configured ? '已配置' : '未配置',
        style: { fontSize: '11px', color: p.configured ? '#3fb950' : 'var(--text-secondary)' },
      });

      const probeBtn = el('button', {
        type: 'button',
        className: 'btn-gray',
        text: '探活',
        style: { padding: '4px 10px', fontSize: '12px' },
        onClick: async () => {
          setText(probeResult, '探测中...');
          try {
            const res = await apiPost('/api/settings/probe-provider', {
              provider_id: p.id,
              base_url: p.base_url,
              api_key: keyInput.value,
            });
            setText(probeResult, res.message || (res.reachable ? '可用' : '不可达'));
            probeResult.style.color = res.reachable ? '#3fb950' : '#f85149';
          } catch (err) {
            setText(probeResult, err.message);
            probeResult.style.color = '#f85149';
          }
        },
      });

      const card = el('div', {
        style: {
          border: '1px solid var(--border)',
          borderRadius: '8px',
          padding: '10px',
          display: 'flex',
          flexDirection: 'column',
          gap: '6px',
        },
      }, [
        el('div', { style: { display: 'flex', justifyContent: 'space-between', alignItems: 'center' } }, [
          el('strong', { text: p.display_name, style: { fontSize: '13px' } }),
          el('span', {
            text: p.supports_vision ? '支持看图' : '纯文本',
            style: { fontSize: '11px', color: p.supports_vision ? 'var(--accent-hover)' : 'var(--text-secondary)' },
          }),
        ]),
        el('div', {
          text: `默认: ${p.default_model} · 上下文: ${Math.round((p.max_context_tokens || 0) / 1000)}K`,
          style: { fontSize: '11px', color: 'var(--text-secondary)' },
        }),
        el('div', { style: { display: 'flex', gap: '6px', alignItems: 'center' } }, [keyInput, probeBtn]),
        probeResult,
      ]);
      list.appendChild(card);
    }
  } catch (err) {
    showToast(`加载路由配置失败: ${err.message}`, 'error');
  }
}

export async function saveRouterConfig() {
  const list = document.getElementById('v1-providers-list');
  const visionSelect = document.getElementById('v1-vision-provider');
  const budgetInput = document.getElementById('v1-daily-budget');
  const offlineInput = document.getElementById('v1-privacy-offline');
  const umiInput = document.getElementById('v1-umi-ocr-url');

  const providerKeys = [];
  if (list) {
    const inputs = list.querySelectorAll('input[data-provider-id]');
    for (const inp of inputs) {
      const pid = inp.dataset.providerId;
      if (pid && pid !== 'ollama') {
        providerKeys.push({ id: pid, api_key: inp.value });
      }
    }
  }

  const budgetVal = budgetInput ? Number.parseInt(budgetInput.value, 10) : 200;
  try {
    await apiPost('/api/settings/config', {
      provider_keys: providerKeys,
      vision_provider_id: visionSelect ? visionSelect.value : '',
      daily_budget: Number.isFinite(budgetVal) ? budgetVal : 200,
      privacy_offline_only: offlineInput ? offlineInput.checked : false,
      umi_ocr_url: umiInput ? umiInput.value.trim() : '',
    });
    showToast('容灾与路由配置已保存（密钥已脱敏）', 'success');
    await Promise.all([loadRouterConfig(), loadEnvironmentHealth()]);
  } catch (err) {
    showToast(`保存失败: ${err.message}`, 'error');
  }
}

export async function runPackImport(filenameHint = null) {
  const textarea = document.getElementById('v1-pack-textarea');
  const formatSel = document.getElementById('v1-pack-format');
  const kindSel = document.getElementById('v1-pack-default-kind');
  const roleSel = document.getElementById('v1-pack-default-role');
  const resultBox = document.getElementById('v1-pack-result');
  if (!textarea || !resultBox) return;

  const content = textarea.value.trim();
  if (!content) {
    showToast('请先选择数据包文件或粘贴内容', 'warn');
    return;
  }

  setText(resultBox, '正在流式切分并写入本机 SQLite 金库...');
  try {
    const res = await apiPost('/api/vault/import-pack', {
      format: formatSel ? formatSel.value : 'auto',
      filename: filenameHint,
      content,
      default_doc_kind: kindSel ? kindSel.value : 'patent',
      default_asset_role: roleSel ? roleSel.value : 'general_ref',
    });
    const msg = `灌库完成：格式 ${res.format_detected} · 成功 ${res.imported_count} 篇 · 跳过 ${res.skipped_count} 篇 · 耗时 ${res.elapsed_ms}ms`;
    setText(resultBox, msg);
    showToast(msg, 'success');
    await Promise.all([loadVaultDocs(), loadEnvironmentHealth()]);
  } catch (err) {
    setText(resultBox, `灌库失败: ${err.message}`);
    showToast(`灌库失败: ${err.message}`, 'error');
  }
}

function buildSelect(options, currentVal, onChange) {
  const sel = el('select', { style: { fontSize: '12px', padding: '3px 6px' } });
  for (const opt of options) {
    sel.appendChild(el('option', { value: opt.value, text: opt.label }));
  }
  sel.value = currentVal;
  sel.addEventListener('change', () => onChange(sel.value));
  return sel;
}

export async function loadVaultDocs() {
  const list = document.getElementById('v1-vault-docs-list');
  const statsEl = document.getElementById('v1-vault-stats');
  const qEl = document.getElementById('v1-vault-search-q');
  const roleEl = document.getElementById('v1-vault-role-filter');
  const kindEl = document.getElementById('v1-vault-kind-filter');
  if (!list) return;

  const params = new URLSearchParams();
  if (qEl && qEl.value.trim()) params.set('q', qEl.value.trim());
  if (roleEl && roleEl.value) params.set('asset_role', roleEl.value);
  if (kindEl && kindEl.value) params.set('doc_kind', kindEl.value);
  params.set('limit', '50');

  try {
    const data = await apiGet(`/api/vault/docs?${params.toString()}`);
    const stats = data.stats || {};
    if (statsEl) {
      setText(
        statsEl,
        `总文献 ${stats.total_docs || 0} · 我方自有 ${stats.own_asset_count || 0} · 已验证实验 ${stats.verified_experiment_count || 0}/${stats.experiment_count || 0} · 特征原子 ${stats.feature_atom_count || 0}`,
      );
    }

    clearChildren(list);
    const docs = Array.isArray(data.docs) ? data.docs : [];
    if (docs.length === 0) {
      list.appendChild(
        el('div', {
          text: '当前筛选条件下暂无金库文献，可通过上方「原生数据包流式导入」或卡片 ① 检索入库。',
          style: { fontSize: '13px', color: 'var(--text-secondary)', padding: '12px 0' },
        }),
      );
      return;
    }

    for (const doc of docs) {
      let curRole = doc.asset_role || 'general_ref';
      let curFiling = doc.filing_status || 'public_disclosed';

      const applyPatch = async () => {
        try {
          await apiPatch('/api/vault/docs', {
            doc_id: doc.doc_id,
            asset_role: curRole,
            filing_status: curFiling,
          });
          showToast(`已更新 ${doc.doc_id} 资产状态`, 'success');
          await loadVaultDocs();
        } catch (err) {
          showToast(`更新失败: ${err.message}`, 'error');
        }
      };

      const roleSel = buildSelect(ROLE_OPTIONS, curRole, (val) => {
        curRole = val;
        applyPatch();
      });
      const filingSel = buildSelect(FILING_OPTIONS, curFiling, (val) => {
        curFiling = val;
        applyPatch();
      });

      const isConfirming = state.pendingDeleteDocId === doc.doc_id;
      const delBtn = el('button', {
        type: 'button',
        className: 'btn-gray',
        text: isConfirming ? '确认删除' : '删除',
        style: {
          padding: '3px 10px',
          fontSize: '12px',
          color: isConfirming ? '#f85149' : undefined,
        },
        onClick: async () => {
          if (state.pendingDeleteDocId !== doc.doc_id) {
            state.pendingDeleteDocId = doc.doc_id;
            await loadVaultDocs();
            return;
          }
          try {
            await apiDelete(`/api/vault/docs?doc_id=${encodeURIComponent(doc.doc_id)}`);
            state.pendingDeleteDocId = null;
            showToast(`已删除 ${doc.doc_id}`, 'success');
            await Promise.all([loadVaultDocs(), loadEnvironmentHealth()]);
          } catch (err) {
            showToast(`删除失败: ${err.message}`, 'error');
          }
        },
      });

      const title = doc.title_zh || doc.title_en || doc.external_id || doc.doc_id;
      const row = el('div', {
        style: {
          border: '1px solid var(--border)',
          borderRadius: '8px',
          padding: '10px 12px',
          display: 'flex',
          flexWrap: 'wrap',
          justifyContent: 'space-between',
          alignItems: 'center',
          gap: '8px',
        },
      }, [
        el('div', { style: { flex: '1', minWidth: '220px' } }, [
          el('div', { text: title, style: { fontWeight: '600', fontSize: '13px' } }),
          el('div', {
            text: `${doc.doc_kind || 'patent'} · ${doc.external_id || doc.doc_id} · ${doc.pub_or_filing_date || '日期未知'}`,
            style: { fontSize: '12px', color: 'var(--text-secondary)', marginTop: '2px' },
          }),
        ]),
        el('div', { style: { display: 'flex', gap: '6px', flexWrap: 'wrap', alignItems: 'center' } }, [
          roleSel,
          filingSel,
          delBtn,
        ]),
      ]);
      list.appendChild(row);
    }
  } catch (err) {
    showToast(`加载金库文献失败: ${err.message}`, 'error');
  }
}

function bindEvents() {
  const healthBtn = document.getElementById('v1-health-refresh-btn');
  if (healthBtn) healthBtn.addEventListener('click', loadEnvironmentHealth);

  const saveRouterBtn = document.getElementById('v1-save-router-btn');
  if (saveRouterBtn) saveRouterBtn.addEventListener('click', saveRouterConfig);

  const packImportBtn = document.getElementById('v1-pack-import-btn');
  if (packImportBtn) packImportBtn.addEventListener('click', () => runPackImport(null));

  const dropzone = document.getElementById('v1-pack-dropzone');
  const fileInput = document.getElementById('v1-pack-file-input');
  const textarea = document.getElementById('v1-pack-textarea');
  const dropLabel = document.getElementById('v1-pack-drop-label');

  const handleFile = async (file) => {
    if (!file || !textarea) return;
    const text = await file.text();
    textarea.value = text;
    if (dropLabel) setText(dropLabel, `已载入文件: ${file.name} (${text.length} 字符)`);
    await runPackImport(file.name);
  };

  if (dropzone && fileInput) {
    dropzone.addEventListener('click', () => fileInput.click());
    dropzone.addEventListener('dragover', (e) => {
      e.preventDefault();
      dropzone.style.borderColor = 'var(--accent-hover)';
    });
    dropzone.addEventListener('dragleave', () => {
      dropzone.style.borderColor = 'var(--border)';
    });
    dropzone.addEventListener('drop', (e) => {
      e.preventDefault();
      dropzone.style.borderColor = 'var(--border)';
      const file = e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files[0];
      if (file) handleFile(file);
    });
    fileInput.addEventListener('change', () => {
      const file = fileInput.files && fileInput.files[0];
      if (file) handleFile(file);
    });
  }

  const vaultReloadBtn = document.getElementById('v1-vault-reload-btn');
  if (vaultReloadBtn) vaultReloadBtn.addEventListener('click', loadVaultDocs);

  const roleFilter = document.getElementById('v1-vault-role-filter');
  if (roleFilter) roleFilter.addEventListener('change', loadVaultDocs);

  const kindFilter = document.getElementById('v1-vault-kind-filter');
  if (kindFilter) kindFilter.addEventListener('change', loadVaultDocs);
}

if (typeof document !== 'undefined') {
  bindEvents();
  Promise.all([loadEnvironmentHealth(), loadRouterConfig(), loadVaultDocs()]);
}
