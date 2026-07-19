// ── Gitka GUI — Main Application ────────────────────────────────

const { invoke } = window.__TAURI__.core;

// ── State ───────────────────────────────────────────────────────

let currentView = 'dashboard';
let repos = [];
let configPath = null;

// ── Init ────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', async () => {
  initNavigation();
  initActions();
  initModal();
  await loadStatus();
});

// ── Navigation ──────────────────────────────────────────────────

function initNavigation() {
  document.querySelectorAll('.nav-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const view = btn.dataset.view;
      switchView(view);
    });
  });
}

function switchView(view) {
  currentView = view;
  document.querySelectorAll('.nav-btn').forEach(b => b.classList.remove('active'));
  document.querySelector(`[data-view="${view}"]`).classList.add('active');
  document.querySelectorAll('.view').forEach(v => v.classList.remove('active'));
  document.getElementById(`view-${view}`).classList.add('active');

  if (view === 'settings') loadConfig();
  if (view === 'repos') renderRepoGrid();
}

// ── Actions ─────────────────────────────────────────────────────

function initActions() {
  document.getElementById('btn-sync-all').addEventListener('click', syncAll);
  document.getElementById('btn-scan').addEventListener('click', scanRepos);
  document.getElementById('btn-import').addEventListener('click', importRepo);
  document.getElementById('btn-save-settings').addEventListener('click', saveSettings);
}

async function syncAll() {
  setStatus('Syncing all repos...');
  try {
    const result = await invoke('sync_repos', { configPath });
    showModal('Sync Complete', `<pre>${escapeHtml(result.output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Sync Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function scanRepos() {
  setStatus('Scanning...');
  try {
    const output = await invoke('scan_repos', { configPath });
    showModal('Scan Results', `<pre>${escapeHtml(output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Scan Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function importRepo() {
  // Simple prompt for path
  const path = prompt('Enter path to git repository:');
  if (!path) return;

  const name = prompt('Repo name (optional, press Enter to skip):') || null;

  setStatus(`Importing ${path}...`);
  try {
    const output = await invoke('import_repo', { configPath, path, name });
    showModal('Import Complete', `<pre>${escapeHtml(output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Import Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function unlockRepo(name) {
  setStatus(`Unlocking ${name}...`);
  try {
    const output = await invoke('unlock_repo', { configPath, repo: name });
    showModal('Unlocked', `<pre>${escapeHtml(output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Unlock Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function lockRepo(name) {
  setStatus(`Locking ${name}...`);
  try {
    const output = await invoke('lock_repo', { configPath, repo: name });
    showModal('Locked', `<pre>${escapeHtml(output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Lock Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function serveRepo(name) {
  setStatus(`Starting serve for ${name}...`);
  try {
    const output = await invoke('serve_repo', { configPath, repo: name });
    showModal('Serving', `<pre>${escapeHtml(output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Serve Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function stopServe(name) {
  setStatus(`Stopping serve for ${name}...`);
  try {
    const output = await invoke('stop_serve', { configPath, repo: name });
    showModal('Server Stopped', `<pre>${escapeHtml(output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Stop Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function verifyRepo(name) {
  setStatus(`Verifying ${name}...`);
  try {
    const output = await invoke('verify_archives', { configPath, repos: [name] });
    showModal('Verify Result', `<pre>${escapeHtml(output)}</pre>`);
  } catch (e) {
    showModal('Verify Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

async function repairRepo(name) {
  setStatus(`Repairing ${name}...`);
  try {
    const output = await invoke('repair_repo', { configPath, repo: name });
    showModal('Repair Result', `<pre>${escapeHtml(output)}</pre>`);
    await loadStatus();
  } catch (e) {
    showModal('Repair Failed', `<p>${escapeHtml(String(e))}</p>`);
  }
}

// ── Status ──────────────────────────────────────────────────────

async function loadStatus() {
  setStatus('Loading status...');
  try {
    repos = await invoke('get_status', { configPath });
    renderDashboard();
    renderRepoGrid();
    updateStats();
    setStatus('Ready');
  } catch (e) {
    // If gitka isn't configured yet, show empty state
    repos = [];
    renderDashboard();
    renderRepoGrid();
    updateStats();
    setStatus('Ready — no backup configured');
  }
}

function updateStats() {
  const total = repos.length;
  const archived = repos.filter(r => r.state === 'Archived').length;
  const extracted = total - archived;

  document.getElementById('stat-total').textContent = total;
  document.getElementById('stat-archived').textContent = archived;
  document.getElementById('stat-extracted').textContent = extracted;

  // Sum up sizes (rough parse)
  let totalMb = 0;
  for (const r of repos) {
    const match = r.archive_size.match(/([\d.]+)/);
    if (match) totalMb += parseFloat(match[1]);
  }
  document.getElementById('stat-size').textContent =
    totalMb >= 1024 ? `${(totalMb / 1024).toFixed(1)} GB` : `${totalMb.toFixed(1)} MB`;
}

// ── Rendering ───────────────────────────────────────────────────

function renderDashboard() {
  const container = document.getElementById('dashboard-repos');

  if (repos.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
        <p>No repositories found. Run <code>gitka scan</code> or import a repo.</p>
      </div>`;
    return;
  }

  container.innerHTML = repos.map(r => repoCardHTML(r)).join('');
  attachCardActions(container);
}

function renderRepoGrid() {
  const container = document.getElementById('repo-grid');

  if (repos.length === 0) {
    container.innerHTML = `<div class="empty-state"><p>No repositories to display.</p></div>`;
    return;
  }

  container.innerHTML = repos.map(r => repoCardHTML(r, true)).join('');
  attachCardActions(container);
}

function repoCardHTML(r, detailed = false) {
  const stateClass = r.state === 'Archived' ? 'state-archived'
    : r.state === 'ExtractedLocal' ? 'state-extracted'
    : 'state-served';

  const stateLabel = r.state === 'Archived' ? 'Archived'
    : r.state === 'ExtractedLocal' ? 'Extracted'
    : 'Served';

  const actions = r.state === 'Archived'
    ? `<button class="btn btn-primary btn-sm" data-action="unlock" data-repo="${r.name}">Unlock</button>
       <button class="btn btn-secondary btn-sm" data-action="serve" data-repo="${r.name}">Serve</button>`
    : `<button class="btn btn-primary btn-sm" data-action="lock" data-repo="${r.name}">Lock</button>
       <button class="btn btn-secondary btn-sm" data-action="stop" data-repo="${r.name}">Stop</button>`;

  return `
    <div class="repo-card" data-name="${r.name}">
      <div class="repo-info">
        <div class="repo-name">${escapeHtml(r.name)}</div>
        <div class="repo-meta">${escapeHtml(r.last_synced || 'never synced')} ${r.session ? '· ' + escapeHtml(r.session) : ''}</div>
      </div>
      <span class="repo-state ${stateClass}">${stateLabel}</span>
      <span class="repo-size">${escapeHtml(r.archive_size || '—')}</span>
      <div class="repo-actions">
        ${actions}
        <button class="btn btn-secondary btn-sm" data-action="verify" data-repo="${r.name}">Verify</button>
        <button class="btn btn-secondary btn-sm" data-action="repair" data-repo="${r.name}">Repair</button>
      </div>
    </div>`;
}

function attachCardActions(container) {
  container.querySelectorAll('[data-action]').forEach(btn => {
    btn.addEventListener('click', () => {
      const action = btn.dataset.action;
      const repo = btn.dataset.repo;
      switch (action) {
        case 'unlock': unlockRepo(repo); break;
        case 'lock':   lockRepo(repo); break;
        case 'serve':  serveRepo(repo); break;
        case 'stop':   stopServe(repo); break;
        case 'verify': verifyRepo(repo); break;
        case 'repair': repairRepo(repo); break;
      }
    });
  });
}

// ── Config ──────────────────────────────────────────────────────

async function loadConfig() {
  try {
    const result = await invoke('get_config', { configPath });
    parseConfig(result.content);
  } catch (e) {
    // Config not available yet
  }
}

function parseConfig(toml) {
  // Simple TOML field extraction
  const get = (key) => {
    const re = new RegExp(`^${key}\\s*=\\s*(.+)`, 'm');
    const m = toml.match(re);
    return m ? m[1].trim().replace(/^["']|["']$/g, '') : null;
  };

  const getBool = (key) => {
    const v = get(key);
    return v === 'true';
  };

  const v = get('github_username'); if (v) document.getElementById('cfg-github-username').value = v;
  const t = get('auth_token'); if (t) document.getElementById('cfg-auth-token').value = t;
  const tier = get('tier'); if (tier) document.getElementById('cfg-tier').value = tier;
  const dict = get('dictionary_size_mb'); if (dict) document.getElementById('cfg-dict-size').value = dict;
  const solid = get('solid'); if (solid) document.getElementById('cfg-solid').value = solid;
  const vol = get('size_mb'); if (vol) document.getElementById('cfg-volume-size').value = vol;
  document.getElementById('cfg-dedup').checked = getBool('dedup');
  document.getElementById('cfg-encryption').checked = getBool('encryption');
  document.getElementById('cfg-recovery').checked = getBool('recovery_records');
  document.getElementById('cfg-verify').checked = getBool('verify_after_sync');
  document.getElementById('cfg-clear').checked = getBool('clear_after_lock');
  const ext = get('target'); if (ext) document.getElementById('cfg-extraction-target').value = ext;
}

async function saveSettings() {
  const sets = [
    ['source.github_username', document.getElementById('cfg-github-username').value],
    ['source.auth_token', document.getElementById('cfg-auth-token').value],
    ['compression.tier', document.getElementById('cfg-tier').value],
    ['compression.dictionary_size_mb', document.getElementById('cfg-dict-size').value],
    ['compression.solid', document.getElementById('cfg-solid').value],
    ['compression.dedup', document.getElementById('cfg-dedup').checked],
    ['toggles.encryption', document.getElementById('cfg-encryption').checked],
    ['toggles.recovery_records', document.getElementById('cfg-recovery').checked],
    ['toggles.verify_after_sync', document.getElementById('cfg-verify').checked],
    ['toggles.clear_after_lock', document.getElementById('cfg-clear').checked],
    ['extraction.target', document.getElementById('cfg-extraction-target').value],
  ];

  const volSize = parseInt(document.getElementById('cfg-volume-size').value) || 0;
  if (volSize > 0) {
    sets.push(['compression.volume_splitting.size_mb', volSize]);
  } else {
    sets.push(['compression.volume_splitting.size_mb', 'off']);
  }

  setStatus('Saving settings...');
  let errors = 0;
  for (const [key, value] of sets) {
    try {
      await invoke('set_config', { configPath, key, value: String(value) });
    } catch (e) {
      errors++;
    }
  }

  setStatus(errors === 0 ? 'Settings saved' : `Saved with ${errors} error(s)`);
}

// ── Modal ───────────────────────────────────────────────────────

function initModal() {
  document.getElementById('modal-close').addEventListener('click', hideModal);
  document.getElementById('modal-overlay').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) hideModal();
  });
}

function showModal(title, bodyHTML) {
  document.getElementById('modal-title').textContent = title;
  document.getElementById('modal-body').innerHTML = bodyHTML;
  document.getElementById('modal-footer').innerHTML =
    '<button class="btn btn-primary btn-sm" onclick="hideModal()">OK</button>';
  document.getElementById('modal-overlay').classList.add('active');
}

window.hideModal = function() {
  document.getElementById('modal-overlay').classList.remove('active');
};

// ── Utilities ───────────────────────────────────────────────────

function setStatus(text) {
  document.getElementById('status-text').textContent = text;
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
