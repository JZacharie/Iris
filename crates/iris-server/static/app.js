let currentSnapshot = null;
let lastRenderedKey = null;
let clockTimer = null;
let dashboardAutoRefreshTimer = null;

function startDashboardAutoRefresh(screenId, refreshSeconds) {
  stopDashboardAutoRefresh();
  const intervalMs = Math.max(15, (refreshSeconds || 60)) * 1000;
  dashboardAutoRefreshTimer = setInterval(() => {
    const img = document.getElementById("dashboard-render-img");
    if (img) {
      img.src = `/api/renders/${encodeURIComponent(screenId)}?t=${Date.now()}`;
    }
  }, intervalMs);
}

function stopDashboardAutoRefresh() {
  if (dashboardAutoRefreshTimer) {
    clearInterval(dashboardAutoRefreshTimer);
    dashboardAutoRefreshTimer = null;
  }
}

async function api(path, method = "GET", body = null) {
  const opts = { method, headers: { "Content-Type": "application/json" } };
  if (body !== null) opts.body = JSON.stringify(body);
  const res = await fetch(path, opts);
  return res.json();
}

let lastReloadCounter = null;

function connectWs() {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${proto}//${location.host}/ws`);
  ws.onmessage = (ev) => {
    try {
      currentSnapshot = JSON.parse(ev.data);
      if (lastReloadCounter !== null && typeof currentSnapshot.reloadCounter === "number" && currentSnapshot.reloadCounter > lastReloadCounter) {
        console.log("Server requested reload, refreshing Iris kiosk page...", currentSnapshot.reloadCounter);
        lastReloadCounter = currentSnapshot.reloadCounter;
        window.location.reload(true);
        return;
      }
      lastReloadCounter = currentSnapshot.reloadCounter;
      render(currentSnapshot);
    } catch (_) {}
  };
  ws.onclose = () => setTimeout(connectWs, 3000);
}

function render(snap) {
  renderHud(snap);
  renderAlert(snap.activeAlert);
  renderStage(snap);
}

const SCREEN_ICONS = {
  ORG_OVERVIEW: "🏠",
  ORG_HOME: "⚡",
  ORG_LIFE_INFRA: "☸️",
  ORG_AGENTS: "🤖",
  ORG_NEWS: "📰",
  ORG_AGENDA: "📅",
  ORG_NOTES: "📌",
  ORG_CLOCK: "🕒",
  OPEN_OBSERVE: "🔭",
  OPENOBSERVE: "🔭",
  GRAFANA: "📊",
  ORG_STREAM_DECK: "🎛️",
  WEB: "🌐"
};

function screenBadge(s, snap) {
  if (s.type === "ORG_LIFE_INFRA") {
    const unread = (snap.comms?.slackUnread || 0) + (snap.comms?.missedCalls || 0) + (snap.comms?.voicemails || 0);
    return unread > 0 ? String(unread) : "";
  }
  if (s.type === "ORG_AGENTS") {
    const reports = snap.agentReports || [];
    const running = reports.filter((r) => r.status === "running").length;
    if (running > 0) return `${running}⏳`;
    return reports.length > 0 ? String(reports.length) : "";
  }
  if (s.type === "ORG_NEWS") {
    const cnt = (snap.infra?.kusanagi?.news || []).length;
    return cnt > 0 ? String(cnt) : "";
  }
  if (s.type === "ORG_NOTES") {
    const todo = (snap.notes || []).filter((n) => !n.done).length;
    return todo > 0 ? String(todo) : "";
  }
  return "";
}

function renderHud(snap) {
  const pills = document.getElementById("screen-pills");
  pills.innerHTML = "";
  const displayedId = snap.displayedScreen?.id;
  const enabledScreens = snap.screens.filter((s) => s.enabled);

  const orgTypes = new Set(["ORG_OVERVIEW", "ORG_HOME", "ORG_LIFE_INFRA", "ORG_NEWS", "ORG_AGENDA", "ORG_NOTES", "ORG_CLOCK"]);
  const groups = [
    { title: "Quotidien & Maison", items: enabledScreens.filter((s) => orgTypes.has(s.type)) },
    { title: "Observabilité & Poste", items: enabledScreens.filter((s) => !orgTypes.has(s.type)) }
  ];

  groups.forEach((grp) => {
    if (!grp.items.length) return;
    const sec = document.createElement("div");
    sec.className = "nav-section";
    sec.innerHTML = `<div class="nav-section-title">${escapeHtml(grp.title)}</div>`;
    grp.items.forEach((s) => {
      const b = document.createElement("button");
      const isActive = s.id === displayedId && !snap.transientScreen;
      b.className = "nav-item" + (isActive ? " active" : "");
      b.title = s.name;
      const icon = SCREEN_ICONS[s.type] || "🖥️";
      const badge = screenBadge(s, snap);
      b.innerHTML = `
        <span class="nav-icon">${icon}</span>
        <span class="nav-text">${escapeHtml(s.name)}</span>
        ${badge ? `<span class="nav-badge">${escapeHtml(badge)}</span>` : ""}
      `;
      b.onclick = () => api("/api/screen/select", "POST", { target: s.id });
      sec.appendChild(b);
    });
    pills.appendChild(sec);
  });

  if (snap.transientScreen) {
    const sec = document.createElement("div");
    sec.className = "nav-section";
    sec.innerHTML = `<div class="nav-section-title">Écran Temporaire</div>`;
    const t = document.createElement("button");
    t.className = "nav-item active";
    t.innerHTML = `<span class="nav-icon">⚡</span><span class="nav-text">${escapeHtml(snap.transientScreen.name)}</span>`;
    sec.appendChild(t);
    pills.appendChild(sec);
  }

  const dot = document.getElementById("mqtt-dot");
  dot.className = "dot " + snap.mqttState;
  document.getElementById("mqtt-label").textContent = `MQTT: ${snap.mqttState}`;
  const rotLabel = document.getElementById("rotation-label");
  const rotIcon = document.getElementById("rotation-icon");
  if (rotLabel) {
    rotLabel.textContent = snap.settings.rotationEnabled ? "Rotation : Active (Pause)" : "Rotation : En pause";
  }
  if (rotIcon) {
    rotIcon.textContent = snap.settings.rotationEnabled ? "⏸️" : "▶️";
  }

  const barthes = snap.barthesState || "DEGRE_ZERO";
  const barthesMeta = {
    DEGRE_ZERO: { icon: "🟢", label: "Degré zéro", desc: "Équilibre silencieux — flux nominal" },
    STUDIUM: { icon: "🔵", label: "Studium", desc: "Attention calme — veille organisationnelle" },
    CONNOTATION: { icon: "🟠", label: "Connotation", desc: "Signal faible — tension ressources ou quota" },
    PRESENCE: { icon: "🟣", label: "Présence", desc: "En communication — Teams / Zoom actif" },
    PUNCTUM: { icon: "🔴", label: "Punctum", desc: "Rupture critique — incident actif" }
  }[barthes] || { icon: "🟢", label: "Degré zéro", desc: "" };
  const pill = document.getElementById("barthes-pill");
  if (pill) {
    pill.className = `barthes-pill barthes-${barthes}`;
    pill.innerHTML = `<span>${barthesMeta.icon}</span><span class="barthes-label-text">${escapeHtml(barthesMeta.label)}</span>`;
    pill.title = barthesMeta.desc;
  }
}

function renderAlert(alert) {
  const banner = document.getElementById("alert-banner");
  if (!alert) {
    banner.classList.remove("visible");
    return;
  }
  banner.classList.add("visible");
  document.getElementById("alert-severity").textContent = alert.severity;
  document.getElementById("alert-title").textContent = alert.title;
  document.getElementById("alert-meta").textContent = `• ${alert.source}`;
  document.getElementById("alert-message").textContent = alert.message || "";
  document.getElementById("btn-alert-ack").style.display = alert.acknowledged ? "none" : "inline-block";
}

const GRAFANA_DASHBOARDS = [
  { label: "☸️ K8S Cluster", url: "https://grafana.zacharie.org/d/b9b5adf3-6cdb-409d-86ae-0482e25fc165/k8s-dashboard" },
  { label: "🖥️ Nodes Full", url: "https://grafana.zacharie.org/d/rYdddlPWk/node-exporter-full" },
  { label: "🚀 ArgoCD", url: "https://grafana.zacharie.org/d/qPkgGHg7k/argocd" },
  { label: "🐘 CloudNativePG", url: "https://grafana.zacharie.org/d/cloudnative-pg/cloudnativepg" },
  { label: "🌐 CoreDNS", url: "https://grafana.zacharie.org/d/wY4blRMGz/coredns" },
  { label: "📡 EMQX MQTT", url: "https://grafana.zacharie.org/d/tjRlQw6Zk/emqx-monitoring" },
  { label: "🐰 RabbitMQ", url: "https://grafana.zacharie.org/d/Kn5xm-gZk/rabbitmq-overview" },
  { label: "🔐 Vault", url: "https://grafana.zacharie.org/d/vaults/hashicorp-vault" },
  { label: "🐝 Cilium", url: "https://grafana.zacharie.org/d/cilium-metrics/cilium-metrics" }
];

const OPENOBSERVE_DASHBOARDS = [
  { label: "🔭 Antigravity Complete", id: "7516898066293608448", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7516898066293608448&folder=default" },
  { label: "📈 Antigravity Metrics", id: "7516898328903180289", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7516898328903180289&folder=default" },
  { label: "📊 Consommation Gemini", id: "7516531994134597633", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7516531994134597633&folder=default" },
  { label: "🤖 LLM Consommation", id: "7516513601809571840", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7516513601809571840&folder=default" },
  { label: "⚡ Pylos AI Gateway", id: "7526637916655718400", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7526637916655718400&folder=default" },
  { label: "☸️ K8s Clusters", id: "7502190376086695936", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7502190376086695936&folder=default" },
  { label: "📦 K8s Objects", id: "7502190373440069632", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7502190373440069632&folder=default" },
  { label: "🏗️ O2 Infra", id: "7502190357904367616", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7502190357904367616&folder=default" },
  { label: "📨 NATS Messaging", id: "7502190361909927936", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7502190361909927936&folder=default" },
  { label: "🔐 Vault & OpenBao", id: "7502197767322759168", url: "https://o2-openobserve.p.zacharie.org/web/dashboards/view?org_identifier=default&dashboard=7502197767322759168&folder=default" }
];

function renderStage(snap) {
  const screen = snap.displayedScreen;
  const stage = document.getElementById("stage");
  if (!screen) {
    stopDashboardAutoRefresh();
    stage.innerHTML = `<div class="org-view"><h2>Aucun écran configuré</h2></div>`;
    return;
  }

  const key = `${screen.id}|${snap.formattedUrl}|${snap.reloadCounter}|${screen.type}`;
  if (screen.type === "GRAFANA") {
    if (lastRenderedKey === key) return;
    lastRenderedKey = key;
    const rawUrl = (!screen.url || screen.url.endsWith("/d/") || screen.url.endsWith("/d"))
      ? GRAFANA_DASHBOARDS[0].url
      : screen.url;
    const directUrl = (!snap.formattedUrl || snap.formattedUrl.includes("/d/?"))
      ? `${GRAFANA_DASHBOARDS[0].url}?kiosk=tv&theme=dark&refresh=30s`
      : snap.formattedUrl;
    const renderImgUrl = `/api/renders/${encodeURIComponent(screen.id)}?t=${Date.now()}`;

    stage.innerHTML = `
      <div class="dashboard-wrap">
        <div class="dashboard-bar">
          <div class="row" style="gap:6px;flex-wrap:nowrap;overflow-x:auto;">
            ${GRAFANA_DASHBOARDS.map((d) => {
              const active = rawUrl.includes(d.url.split("/d/")[1]?.split("/")[0] || d.url);
              return `<button class="pill grafana-preset ${active ? "active" : ""}" data-url="${escapeHtml(d.url)}">${escapeHtml(d.label)}</button>`;
            }).join("")}
          </div>
          <div class="row" style="gap:8px;flex-shrink:0;">
            <button class="btn btn-force-render" id="btn-force-render-dash" style="font-size:12px;padding:5px 10px;">↻ Forcer le rendu</button>
            <span class="render-badge">⚡ Pré-rendu K8s</span>
            <a class="btn" href="${escapeHtml(directUrl)}" target="_blank" rel="noopener" style="text-decoration:none;white-space:nowrap;font-size:12px;padding:5px 10px;">↗ Ouvrir en direct</a>
          </div>
        </div>
        <div class="dashboard-img-container">
          <img id="dashboard-render-img" class="dashboard-render-img" src="${renderImgUrl}" alt="Grafana Pré-rendu" />
        </div>
      </div>
    `;

    const renderImgEl = document.getElementById("dashboard-render-img");
    if (renderImgEl) {
      renderImgEl.onload = () => {
        renderImgEl.style.opacity = "1";
        const fb = document.getElementById("btn-force-render-dash");
        if (fb && fb.disabled) {
          fb.textContent = "✓ À jour";
          setTimeout(() => {
            fb.disabled = false;
            fb.textContent = "↻ Forcer le rendu";
          }, 1500);
        }
      };
    }

    stage.querySelectorAll(".grafana-preset").forEach((btn) => {
      btn.onclick = async () => {
        const newUrl = btn.dataset.url;
        if (!newUrl) return;
        stage.querySelectorAll(".grafana-preset").forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        const imgPre = document.getElementById("dashboard-render-img");
        if (imgPre) imgPre.style.opacity = "0.45";
        const fbPre = document.getElementById("btn-force-render-dash");
        if (fbPre) {
          fbPre.disabled = true;
          fbPre.textContent = "⏳ Rendu K8s...";
        }
        await api("/api/screens", "POST", {
          ...screen,
          url: newUrl
        });
        const fb = document.getElementById("btn-force-render-dash");
        if (fb) {
          fb.disabled = true;
          fb.textContent = "⏳ Rendu K8s...";
        }
        try {
          await api(`/api/renders/${encodeURIComponent(screen.id)}/refresh`, "POST");
          const img = document.getElementById("dashboard-render-img");
          if (img) img.src = `/api/renders/${encodeURIComponent(screen.id)}?t=${Date.now()}`;
        } catch (e) {
          const fbErr = document.getElementById("btn-force-render-dash");
          if (fbErr) {
            fbErr.disabled = false;
            fbErr.textContent = "⚠️ Erreur";
            setTimeout(() => {
              fbErr.disabled = false;
              fbErr.textContent = "↻ Forcer le rendu";
            }, 3000);
          }
        }
      };
    });

    const forceBtn = document.getElementById("btn-force-render-dash");
    if (forceBtn) {
      forceBtn.onclick = async () => {
        forceBtn.disabled = true;
        forceBtn.textContent = "⏳ Rendu K8s...";
        const imgPre = document.getElementById("dashboard-render-img");
        if (imgPre) imgPre.style.opacity = "0.45";
        try {
          await api(`/api/renders/${encodeURIComponent(screen.id)}/refresh`, "POST");
          const img = document.getElementById("dashboard-render-img");
          if (img) img.src = `/api/renders/${encodeURIComponent(screen.id)}?t=${Date.now()}`;
        } catch (e) {
          forceBtn.disabled = false;
          forceBtn.textContent = "⚠️ Erreur";
          if (imgPre) imgPre.style.opacity = "1";
          setTimeout(() => {
            forceBtn.disabled = false;
            forceBtn.textContent = "↻ Forcer le rendu";
          }, 3000);
        }
      };
    }

    startDashboardAutoRefresh(screen.id, screen.autoRefreshSeconds);
    return;
  }

  if (screen.type === "OPEN_OBSERVE" || screen.type === "OPENOBSERVE") {
    if (lastRenderedKey === key) return;
    lastRenderedKey = key;
    const rawUrl = (!screen.url || screen.url.trim().length === 0 || screen.url.endsWith("/dashboards") || screen.url.endsWith("/dashboards/") || screen.url.includes("k8s-etcd-objects-inventory"))
      ? OPENOBSERVE_DASHBOARDS[0].id
      : screen.url;
    const directUrl = (snap.formattedUrl && !snap.formattedUrl.endsWith("/dashboards") && !snap.formattedUrl.endsWith("/dashboards/") && !snap.formattedUrl.includes("k8s-etcd-objects-inventory"))
      ? snap.formattedUrl
      : (screen.url && screen.url.startsWith("http") && !screen.url.endsWith("/dashboards") && !screen.url.endsWith("/dashboards/") && !screen.url.includes("k8s-etcd-objects-inventory"))
      ? screen.url
      : OPENOBSERVE_DASHBOARDS[0].url;
    const renderImgUrl = `/api/renders/${encodeURIComponent(screen.id)}?t=${Date.now()}`;

    stage.innerHTML = `
      <div class="dashboard-wrap">
        <div class="dashboard-bar">
          <div class="row" style="gap:6px;flex-wrap:nowrap;overflow-x:auto;">
            ${OPENOBSERVE_DASHBOARDS.map((d) => {
              const active = rawUrl.includes(d.id);
              return `<button class="pill openobserve-preset ${active ? "active" : ""}" data-url="${escapeHtml(d.url)}">${escapeHtml(d.label)}</button>`;
            }).join("")}
          </div>
          <div class="row" style="gap:8px;flex-shrink:0;">
            <button class="btn btn-force-render" id="btn-force-render-dash" style="font-size:12px;padding:5px 10px;">↻ Forcer le rendu</button>
            <span class="render-badge">⚡ Pré-rendu K8s</span>
            <a class="btn" href="${escapeHtml(directUrl)}" target="_blank" rel="noopener" style="text-decoration:none;white-space:nowrap;font-size:12px;padding:5px 10px;">↗ Ouvrir en direct</a>
          </div>
        </div>
        <div class="dashboard-img-container">
          <img id="dashboard-render-img" class="dashboard-render-img" src="${renderImgUrl}" alt="OpenObserve Pré-rendu" />
        </div>
      </div>
    `;

    const renderImgEl = document.getElementById("dashboard-render-img");
    if (renderImgEl) {
      renderImgEl.onload = () => {
        renderImgEl.style.opacity = "1";
        const fb = document.getElementById("btn-force-render-dash");
        if (fb && fb.disabled) {
          fb.textContent = "✓ À jour";
          setTimeout(() => {
            fb.disabled = false;
            fb.textContent = "↻ Forcer le rendu";
          }, 1500);
        }
      };
    }

    stage.querySelectorAll(".openobserve-preset").forEach((btn) => {
      btn.onclick = async () => {
        const newUrl = btn.dataset.url;
        if (!newUrl) return;
        stage.querySelectorAll(".openobserve-preset").forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        const imgPre = document.getElementById("dashboard-render-img");
        if (imgPre) imgPre.style.opacity = "0.45";
        const fbPre = document.getElementById("btn-force-render-dash");
        if (fbPre) {
          fbPre.disabled = true;
          fbPre.textContent = "⏳ Rendu K8s...";
        }
        await api("/api/screens", "POST", {
          ...screen,
          url: newUrl
        });
        const fb = document.getElementById("btn-force-render-dash");
        if (fb) {
          fb.disabled = true;
          fb.textContent = "⏳ Rendu K8s...";
        }
        try {
          await api(`/api/renders/${encodeURIComponent(screen.id)}/refresh`, "POST");
          const img = document.getElementById("dashboard-render-img");
          if (img) img.src = `/api/renders/${encodeURIComponent(screen.id)}?t=${Date.now()}`;
        } catch (e) {
          const fbErr = document.getElementById("btn-force-render-dash");
          if (fbErr) {
            fbErr.disabled = false;
            fbErr.textContent = "⚠️ Erreur";
            setTimeout(() => {
              fbErr.disabled = false;
              fbErr.textContent = "↻ Forcer le rendu";
            }, 3000);
          }
        }
      };
    });

    const forceBtn = document.getElementById("btn-force-render-dash");
    if (forceBtn) {
      forceBtn.onclick = async () => {
        forceBtn.disabled = true;
        forceBtn.textContent = "⏳ Rendu K8s...";
        const imgPre = document.getElementById("dashboard-render-img");
        if (imgPre) imgPre.style.opacity = "0.45";
        try {
          await api(`/api/renders/${encodeURIComponent(screen.id)}/refresh`, "POST");
          const img = document.getElementById("dashboard-render-img");
          if (img) img.src = `/api/renders/${encodeURIComponent(screen.id)}?t=${Date.now()}`;
        } catch (e) {
          forceBtn.disabled = false;
          forceBtn.textContent = "⚠️ Erreur";
          if (imgPre) imgPre.style.opacity = "1";
          setTimeout(() => {
            forceBtn.disabled = false;
            forceBtn.textContent = "↻ Forcer le rendu";
          }, 3000);
        }
      };
    }

    startDashboardAutoRefresh(screen.id, screen.autoRefreshSeconds);
    return;
  }

  if (screen.type === "WEB") {
    stopDashboardAutoRefresh();
    if (lastRenderedKey === key) return;
    lastRenderedKey = key;
    if (!snap.formattedUrl) {
      stage.innerHTML = `<div class="org-view"><h2>Aucune URL configurée pour ${screen.name}</h2></div>`;
      return;
    }
    stage.innerHTML = `<iframe class="dashboard-frame" src="${snap.formattedUrl}" allow="fullscreen"></iframe>`;
    return;
  }

  stopDashboardAutoRefresh();
  lastRenderedKey = null;
  if (screen.type === "ORG_OVERVIEW") renderOrgOverview(stage, snap);
  else if (screen.type === "ORG_HOME") renderOrgHome(stage, snap);
  else if (screen.type === "ORG_LIFE_INFRA") renderLifeInfra(stage, snap);
  else if (screen.type === "ORG_AGENTS") renderOrgAgents(stage, snap);
  else if (screen.type === "ORG_NEWS") renderOrgNews(stage, snap);
  else if (screen.type === "ORG_STREAM_DECK") renderStreamDeck(stage, snap);
  else if (screen.type === "ORG_AGENDA") renderOrgAgenda(stage, snap);
  else if (screen.type === "ORG_NOTES") renderOrgNotes(stage, snap);
  else if (screen.type === "ORG_CLOCK") renderOrgClock(stage);
}

let activeAgentsFilter = "ALL";
let showTriggerAgentForm = false;
let expandedReportIds = new Set();
let collapsedReportIds = new Set();
let agentTriggerStatusMsg = "";

const UPGRADE_AGENTS = new Set(["antigravity-pr-updater", "gitops-upgrader", "github-pr-manager"]);
const DEV_AGENTS = new Set(["antigravity-dev", "antigravity-cli"]);

function formatAgentStatusMeta(r) {
  if (r.status === "running") {
    return { css: "running", pillText: "⏳ En cours" };
  }
  if (r.status === "succeeded") {
    if (r.isRealSuccess === false) {
      return { css: "warning", pillText: "⚠️ Succès partiel" };
    }
    return { css: "succeeded", pillText: "✅ Succès" };
  }
  return { css: "failed", pillText: "❌ Échec" };
}

function formatDurationSecs(secs) {
  if (secs == null || Number.isNaN(Number(secs))) return "—";
  const s = Math.max(0, Math.round(Number(secs)));
  if (s < 60) return `${s}s`;
  const mins = Math.floor(s / 60);
  const rem = s % 60;
  return `${mins}m ${rem}s`;
}

function renderRecentAgentReportsCard(snap, btnId, hideIfEmpty) {
  const reports = snap.agentReports || [];
  if (hideIfEmpty && !reports.length) return "";
  return `
    <div class="card">
      <div class="row" style="justify-content:space-between;align-items:center;">
        <h3>🤖 Derniers Rapports d'Agents & Upgrades (${reports.length})</h3>
        <button class="btn" id="${escapeHtml(btnId)}" style="padding:4px 10px;font-size:12px;">Tout voir →</button>
      </div>
      <div style="margin-top:8px;">
        ${reports.length ? reports.slice(0, 3).map((r) => {
          const meta = formatAgentStatusMeta(r);
          const ts = formatShortNewsDate(r.completedAt || r.startedAt);
          const summary = r.summaryNote || r.prompt || r.id;
          return `
            <div style="padding:7px 0;border-bottom:1px solid var(--border);font-size:13px;">
              <div class="row" style="justify-content:space-between;gap:8px;">
                <div class="row" style="gap:6px;">
                  <span class="report-status-pill ${meta.css}">${meta.pillText}</span>
                  <strong>${escapeHtml(r.agentName || "agent")}</strong>
                  ${r.model ? `<span class="badge">${escapeHtml(r.model)}</span>` : ""}
                </div>
                <span style="color:var(--muted);font-size:11px;">${escapeHtml(ts || "—")}</span>
              </div>
              <div style="margin-top:4px;color:var(--muted);font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;" title="${escapeHtml(summary)}">${escapeHtml(summary)}</div>
            </div>
          `;
        }).join("") : "<span style='color:var(--muted)'>Aucun rapport d'agent synchronisé pour le moment.</span>"}
      </div>
    </div>
  `;
}

function matchesAgentFilter(r, filterKey) {
  if (filterKey === "ALL") return true;
  if (filterKey === "UPGRADES") {
    return UPGRADE_AGENTS.has(r.agentName) || /upgrade|pr/i.test(r.agentName || "");
  }
  if (filterKey === "DEV") {
    return DEV_AGENTS.has(r.agentName) || /dev|cli/i.test(r.agentName || "");
  }
  if (filterKey === "SUCCEEDED") {
    return r.status === "succeeded" && r.isRealSuccess !== false;
  }
  if (filterKey === "FAILED") {
    return r.status === "failed" || (r.status === "succeeded" && r.isRealSuccess === false);
  }
  return r.agentName === filterKey;
}

function renderOrgAgents(stage, snap) {
  const reports = snap.agentReports || [];
  const openPrs = snap.infra?.openPrs || [];
  const totalCount = reports.length;
  const succeededCount = reports.filter((r) => r.status === "succeeded" && r.isRealSuccess !== false).length;
  const failedCount = reports.filter((r) => r.status === "failed" || (r.status === "succeeded" && r.isRealSuccess === false)).length;
  const runningCount = reports.filter((r) => r.status === "running").length;

  const latestUpgrade = reports.find((r) => UPGRADE_AGENTS.has(r.agentName) || /upgrade|pr/i.test(r.agentName || ""));
  const latestDev = reports.find((r) => DEV_AGENTS.has(r.agentName) || /dev|cli/i.test(r.agentName || ""));
  const latestUpgradeMeta = latestUpgrade ? formatAgentStatusMeta(latestUpgrade) : null;
  const latestDevMeta = latestDev ? formatAgentStatusMeta(latestDev) : null;

  const availableAgents = Array.from(new Set(reports.map((r) => r.agentName).filter(Boolean))).sort();
  const staticFilters = [
    { key: "ALL", label: `Tous (${totalCount})` },
    { key: "UPGRADES", label: "🔄 Upgrades & PRs" },
    { key: "DEV", label: "🛠️ Dev Antigravity" },
    { key: "SUCCEEDED", label: `✅ Succès (${succeededCount})` },
    { key: "FAILED", label: `⚠️ Échecs / Partiels (${failedCount})` }
  ];
  const validFilterKeys = new Set([...staticFilters.map((f) => f.key), ...availableAgents]);
  if (!validFilterKeys.has(activeAgentsFilter)) {
    activeAgentsFilter = "ALL";
  }

  const filtered = reports.filter((r) => matchesAgentFilter(r, activeAgentsFilter));

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <div>
          <h2>🤖 Rapports Agents & Upgrades (Amaraki / Antigravity)</h2>
          <div style="color:var(--muted);font-size:13px;">Historique persistant 24h & temps réel des missions PRs, upgrades GitOps et développements Antigravity sur jo3</div>
        </div>
        <div class="row">
          <button class="btn primary" id="btn-launch-pr-upgrade">🚀 Lancer Upgrade PRs</button>
          <button class="btn" id="btn-toggle-agent-form">${showTriggerAgentForm ? "✕ Fermer formulaire" : "🛠️ Nouvelle Mission Antigravity"}</button>
          <button class="btn" id="btn-agents-refresh">↻ Synchroniser</button>
        </div>
      </div>

      ${agentTriggerStatusMsg ? `
        <div class="card" style="margin-bottom:14px;border-left:4px solid var(--primary);padding:10px 14px;">
          <div class="row" style="justify-content:space-between;">
            <span style="font-size:13px;">${escapeHtml(agentTriggerStatusMsg)}</span>
            <button class="btn" id="btn-clear-agent-msg" style="padding:3px 9px;font-size:11px;">Fermer</button>
          </div>
        </div>
      ` : ""}

      ${showTriggerAgentForm ? `
        <div class="card" id="agent-trigger-form" style="margin-bottom:16px;">
          <h3 style="margin-bottom:10px;">🛠️ Lancer une mission Antigravity sur jo3 via Amaraki</h3>
          <div class="row" style="align-items:flex-end;">
            <div class="field" style="min-width:220px;">
              <label>Agent cible</label>
              <select id="agent-trigger-name">
                <option value="antigravity-pr-updater">antigravity-pr-updater (PRs & Dépendances)</option>
                <option value="antigravity-dev">antigravity-dev (Développement & Correctifs)</option>
                <option value="antigravity-cli">antigravity-cli (Audit / CLI Antigravity)</option>
                <option value="gitops-upgrader">gitops-upgrader (Helms & Manifests jo3)</option>
                <option value="github-pr-manager">github-pr-manager (Revue & Merge PRs)</option>
                <option value="audit-cluster">audit-cluster (Diagnostic Cluster jo3)</option>
              </select>
            </div>
            <div class="field" style="flex:1;min-width:280px;">
              <label>Prompt / Instructions de mission</label>
              <input id="agent-trigger-prompt" placeholder="Ex: Mets à jour les PRs Dependabot sur jo3, Amaraki et Iris puis génère un rapport." />
            </div>
            <button class="btn primary" id="btn-submit-agent-trigger">Exécuter</button>
          </div>
        </div>
      ` : ""}

      <div class="kpi-grid-4">
        <div class="kpi-card">
          <div class="kpi-label"><span>Missions & Upgrades</span><span>🤖</span></div>
          <div class="kpi-value">${totalCount}</div>
          <div class="kpi-sub">✅ ${succeededCount} succès • ⚠️/❌ ${failedCount} • ⏳ ${runningCount} en cours</div>
        </div>
        <div class="kpi-card">
          <div class="kpi-label"><span>Dernier Upgrade PRs</span><span>🔄</span></div>
          <div class="kpi-value" style="font-size:20px;">${latestUpgradeMeta ? latestUpgradeMeta.pillText : "—"}</div>
          <div class="kpi-sub">${latestUpgrade ? `${escapeHtml(latestUpgrade.agentName)} • ${escapeHtml(formatShortNewsDate(latestUpgrade.completedAt || latestUpgrade.startedAt) || "—")}` : "Aucun upgrade récent"}</div>
        </div>
        <div class="kpi-card">
          <div class="kpi-label"><span>Dernier Dev Antigravity</span><span>🛠️</span></div>
          <div class="kpi-value" style="font-size:20px;">${latestDevMeta ? latestDevMeta.pillText : "—"}</div>
          <div class="kpi-sub">${latestDev ? `${escapeHtml(latestDev.agentName)} • ${escapeHtml(formatShortNewsDate(latestDev.completedAt || latestDev.startedAt) || "—")}` : "Aucune mission dev récente"}</div>
        </div>
        <div class="kpi-card">
          <div class="kpi-label"><span>PRs GitHub Ouvertes</span><span>🔀</span></div>
          <div class="kpi-value" style="color:${openPrs.length > 0 ? "var(--primary)" : "var(--ok)"};">${openPrs.length}</div>
          <div class="kpi-sub">Dépôts suivis par Iris</div>
        </div>
      </div>

      <div class="news-filter-bar">
        ${staticFilters.map((f) => `
          <button class="btn ${activeAgentsFilter === f.key ? "primary" : ""} agent-filter-btn" data-filter="${escapeHtml(f.key)}">${escapeHtml(f.label)}</button>
        `).join("")}
        ${availableAgents.map((agent) => `
          <button class="btn ${activeAgentsFilter === agent ? "primary" : ""} agent-filter-btn" data-filter="${escapeHtml(agent)}">🤖 ${escapeHtml(agent)}</button>
        `).join("")}
      </div>

      <div class="reports-list">
        ${filtered.length ? filtered.map((r, idx) => {
          const meta = formatAgentStatusMeta(r);
          const isExpanded = expandedReportIds.has(r.id) || (idx < 2 && !collapsedReportIds.has(r.id));
          const startedLabel = formatShortNewsDate(r.startedAt) || r.startedAt || "—";
          const completedLabel = r.completedAt ? (formatShortNewsDate(r.completedAt) || r.completedAt) : "En cours";
          const outputText = (r.reportOutput || "").trim();
          return `
            <div class="report-card status-${meta.css}">
              <div class="row" style="justify-content:space-between;align-items:flex-start;">
                <div class="row" style="gap:8px;">
                  <span class="report-status-pill ${meta.css}">${meta.pillText}</span>
                  <strong style="font-size:15px;">${escapeHtml(r.agentName || "agent")}</strong>
                  ${r.model ? `<span class="badge">🧠 ${escapeHtml(r.model)}</span>` : ""}
                  ${r.channel ? `<span class="badge" style="background:rgba(148,163,184,0.16);color:var(--text);">📡 ${escapeHtml(r.channel)}</span>` : ""}
                  <span style="color:var(--muted);font-size:12px;">Job: <code>${escapeHtml(r.id)}</code></span>
                </div>
                <div class="row" style="gap:10px;color:var(--muted);font-size:12px;">
                  <span>⏱️ ${escapeHtml(formatDurationSecs(r.durationSecs))}</span>
                  <span>🚀 ${escapeHtml(startedLabel)}</span>
                  <span>🏁 ${escapeHtml(completedLabel)}</span>
                </div>
              </div>
              ${r.prompt ? `<div class="report-prompt"><strong>Mission :</strong> ${escapeHtml(r.prompt)}</div>` : ""}
              ${r.summaryNote ? `<div style="font-size:13px;color:var(--text);"><strong>Résumé :</strong> ${escapeHtml(r.summaryNote)}</div>` : ""}
              ${outputText ? `
                <div>
                  <div class="row" style="justify-content:space-between;margin-bottom:6px;">
                    <span style="font-size:12px;color:var(--muted);font-weight:600;">📄 Rapport complet d'exécution</span>
                    <button class="btn toggle-report-btn" data-report-id="${escapeHtml(r.id)}" data-default-open="${idx < 2 ? "1" : "0"}" style="padding:4px 10px;font-size:12px;">
                      ${isExpanded ? "▲ Réduire le rapport" : "▼ Afficher le rapport complet"}
                    </button>
                  </div>
                  ${isExpanded ? `<pre class="report-pre">${escapeHtml(outputText)}</pre>` : ""}
                </div>
              ` : `<div style="font-size:12px;color:var(--muted);">${r.status === "running" ? "⏳ Collecte du rapport en cours après exécution du pod…" : "Aucun log ou rapport détaillé disponible."}</div>`}
            </div>
          `;
        }).join("") : `
          <div class="card">
            <span style="color:var(--muted);">Aucun rapport d'agent ne correspond à ce filtre.</span>
          </div>
        `}
      </div>
    </div>
  `;

  const btnClearMsg = document.getElementById("btn-clear-agent-msg");
  if (btnClearMsg) {
    btnClearMsg.onclick = () => {
      agentTriggerStatusMsg = "";
      renderOrgAgents(stage, currentSnapshot || snap);
    };
  }

  const btnLaunchUpgrade = document.getElementById("btn-launch-pr-upgrade");
  if (btnLaunchUpgrade) {
    btnLaunchUpgrade.onclick = async () => {
      btnLaunchUpgrade.disabled = true;
      btnLaunchUpgrade.textContent = "⏳ Lancement en cours…";
      try {
        const res = await api("/api/agents/trigger", "POST", {
          agentName: "antigravity-pr-updater",
          prompt: "Lance l'audit et la mise à jour autonome des PRs et dépendances des dépôts GitHub de JZacharie (jo3, helms, Amaraki, Iris, Kusanagi) avec rapport détaillé.",
          channel: "iris-ui"
        });
        if (res?.error) {
          agentTriggerStatusMsg = `❌ Erreur déclenchement Upgrade PRs : ${res.error}`;
        } else {
          agentTriggerStatusMsg = `🚀 Mission antigravity-pr-updater lancée (Job: ${res?.jobId || "créé"})`;
          if (res?.snapshot) {
            currentSnapshot = res.snapshot;
          }
        }
      } catch (err) {
        agentTriggerStatusMsg = `❌ Erreur réseau déclenchement : ${err?.message || err}`;
      }
      renderOrgAgents(stage, currentSnapshot || snap);
    };
  }

  const btnToggleForm = document.getElementById("btn-toggle-agent-form");
  if (btnToggleForm) {
    btnToggleForm.onclick = () => {
      showTriggerAgentForm = !showTriggerAgentForm;
      renderOrgAgents(stage, currentSnapshot || snap);
    };
  }

  const btnSubmitTrigger = document.getElementById("btn-submit-agent-trigger");
  if (btnSubmitTrigger) {
    btnSubmitTrigger.onclick = async () => {
      const agentName = document.getElementById("agent-trigger-name")?.value || "antigravity-dev";
      const prompt = document.getElementById("agent-trigger-prompt")?.value.trim() || "";
      if (!prompt) {
        agentTriggerStatusMsg = "⚠️ Veuillez saisir un prompt pour la mission Antigravity.";
        renderOrgAgents(stage, currentSnapshot || snap);
        return;
      }
      btnSubmitTrigger.disabled = true;
      btnSubmitTrigger.textContent = "⏳ Lancement…";
      try {
        const res = await api("/api/agents/trigger", "POST", { agentName, prompt, channel: "iris-ui" });
        if (res?.error) {
          agentTriggerStatusMsg = `❌ Erreur : ${res.error}`;
        } else {
          agentTriggerStatusMsg = `🚀 Mission ${agentName} lancée (Job: ${res?.jobId || "créé"})`;
          showTriggerAgentForm = false;
          if (res?.snapshot) {
            currentSnapshot = res.snapshot;
          }
        }
      } catch (err) {
        agentTriggerStatusMsg = `❌ Erreur réseau : ${err?.message || err}`;
      }
      renderOrgAgents(stage, currentSnapshot || snap);
    };
  }

  const btnRefresh = document.getElementById("btn-agents-refresh");
  if (btnRefresh) {
    btnRefresh.onclick = async () => {
      btnRefresh.textContent = "⏳ Synchro…";
      await api("/api/org/refresh", "POST");
    };
  }

  document.querySelectorAll(".agent-filter-btn").forEach((btn) => {
    btn.onclick = () => {
      activeAgentsFilter = btn.dataset.filter || "ALL";
      renderOrgAgents(stage, currentSnapshot || snap);
    };
  });

  document.querySelectorAll(".toggle-report-btn").forEach((btn) => {
    btn.onclick = () => {
      const id = btn.dataset.reportId;
      const defaultOpen = btn.dataset.defaultOpen === "1";
      const currentlyOpen = expandedReportIds.has(id) || (defaultOpen && !collapsedReportIds.has(id));
      if (currentlyOpen) {
        expandedReportIds.delete(id);
        collapsedReportIds.add(id);
      } else {
        collapsedReportIds.delete(id);
        expandedReportIds.add(id);
      }
      renderOrgAgents(stage, currentSnapshot || snap);
    };
  });
}

let selectedNewsSource = "ALL";

function formatShortNewsDate(iso) {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString("fr-FR", { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" });
}

function renderOrgNews(stage, snap) {
  const kusanagi = snap.infra?.kusanagi;
  const items = kusanagi?.news || [];
  const availableSources = Array.from(new Set(items.map((i) => i.source).filter(Boolean))).sort();
  if (selectedNewsSource !== "ALL" && !availableSources.includes(selectedNewsSource)) {
    selectedNewsSource = "ALL";
  }
  const filtered = selectedNewsSource === "ALL"
    ? items
    : items.filter((i) => i.source === selectedNewsSource);

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:14px;">
        <div>
          <h2>📰 Actualités Kusanagi</h2>
          <div style="color:var(--muted);font-size:13px;">${items.length} articles récents • ${availableSources.length} sources actives</div>
        </div>
        <button class="btn primary" id="btn-refresh-news">↻ Actualiser</button>
      </div>

      <div class="news-filter-bar">
        <button class="pill ${selectedNewsSource === "ALL" ? "active" : ""}" data-src="ALL">Toutes (${items.length})</button>
        ${availableSources.map((src) => {
          const cnt = items.filter((i) => i.source === src).length;
          return `<button class="pill ${selectedNewsSource === src ? "active" : ""}" data-src="${escapeHtml(src)}">${escapeHtml(src)} (${cnt})</button>`;
        }).join("")}
      </div>

      ${filtered.length ? `
        <div class="news-grid">
          ${filtered.map((n) => `
            <article class="news-card">
              <div>
                <div class="row" style="justify-content:space-between;margin-bottom:8px;">
                  <span class="badge">${escapeHtml(n.icon || "🗞️")} ${escapeHtml(n.source || "news")}</span>
                  <span style="color:var(--muted);font-size:11px;">${escapeHtml(formatShortNewsDate(n.publishedAt))}</span>
                </div>
                <a class="news-title" href="${escapeHtml(n.url)}" target="_blank" rel="noopener">${escapeHtml(n.title)}</a>
                ${n.description ? `<p class="news-desc" style="margin-top:8px;">${escapeHtml(n.description)}</p>` : ""}
              </div>
              ${n.tags?.length ? `
                <div class="row" style="gap:6px;">
                  ${n.tags.slice(0, 3).map((t) => `<span style="font-size:10px;padding:2px 7px;border-radius:999px;background:var(--bg);border:1px solid var(--border);color:var(--muted);">${escapeHtml(t)}</span>`).join("")}
                </div>
              ` : ""}
            </article>
          `).join("")}
        </div>
      ` : `
        <div class="card">
          <p style="color:var(--muted);">Aucune actualité disponible pour le moment (en attente de Kusanagi /api/news).</p>
        </div>
      `}
    </div>
  `;

  document.getElementById("btn-refresh-news").onclick = () => api("/api/org/refresh", "POST");
  document.querySelectorAll(".news-filter-bar .pill").forEach((btn) => {
    btn.onclick = () => {
      selectedNewsSource = btn.dataset.src || "ALL";
      renderOrgNews(stage, snap);
    };
  });
}

function pctColor(pct) {
  if (pct >= 85) return "var(--crit)";
  if (pct >= 65) return "var(--warn)";
  return "var(--ok)";
}

function formatUptime(secs) {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  return d > 0 ? `${d}j ${h}h` : `${h}h`;
}

function renderOrgHome(stage, snap) {
  const ha = snap.homeAssistant;
  if (!ha || !ha.connected) {
    stage.innerHTML = `
      <div class="org-view">
        <div class="row" style="justify-content:space-between;margin-bottom:16px;">
          <div>
            <h2>🏠 Maison & Énergie (Home Assistant)</h2>
            <div style="color:var(--muted);font-size:13px;">Connexion à vha.zacharie.org en attente (vérifiez IRIS_HOME_ASSISTANT_TOKEN)</div>
          </div>
          <button class="btn primary" id="btn-refresh-ha">↻ Actualiser</button>
        </div>
        <div class="card">
          <p style="color:var(--muted);">Les capteurs Home Assistant (Volvo EX40, Solaire Enphase, Confort, NAS JoNAS, Prises, Volets) apparaîtront ici dès que le token est configuré.</p>
        </div>
      </div>
    `;
    const btn = document.getElementById("btn-refresh-ha");
    if (btn) btn.onclick = () => api("/api/org/refresh", "POST");
    return;
  }

  const people = ha.people || [];
  const car = ha.vehicle;
  const energy = ha.energy || {};
  const plugs = energy.plugs || [];
  const maxPlugW = Math.max(100, ...plugs.map((p) => p.powerW || 0));
  const totalPlugW = plugs.reduce((acc, p) => acc + Math.max(0, p.powerW || 0), 0);
  const climate = ha.climate || [];
  const nas = ha.nas;
  const dev = ha.devices || {};

  const carBat = car?.batteryPct ?? 0;
  const carBatColor = carBat <= 20 ? "var(--crit)" : carBat <= 45 ? "var(--warn)" : "var(--ok)";

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:14px;">
        <div>
          <h2>🏠 Maison & Énergie</h2>
          <div class="presence-strip" style="margin-top:8px;">
            ${people.map((p) => `
              <span class="presence-pill ${p.isHome ? "home" : ""}">
                <span>${p.isHome ? "🟢" : "⚪"}</span>
                <span>${escapeHtml(p.name)}</span>
              </span>
            `).join("")}
            <span class="presence-pill ${dev.alarmState === "armed_away" ? "home" : ""}">
              🛡️ Alarme : ${escapeHtml(dev.alarmState || "—")}
            </span>
          </div>
        </div>
        <button class="btn primary" id="btn-refresh-ha">↻ Actualiser</button>
      </div>

      <div class="kpi-grid-4">
        <div class="kpi-card">
          <div class="kpi-label"><span>🚙 Volvo EX40</span><span>${escapeHtml(car?.lockState || "—")}</span></div>
          <div class="kpi-value" style="color:${carBatColor}">${car?.batteryPct != null ? `${car.batteryPct}%` : "—"}</div>
          <div class="bar-track"><div class="bar-fill" style="width:${carBat}%;background:${carBatColor};"></div></div>
          <div class="kpi-sub">${car?.rangeKm != null ? `${car.rangeKm} km` : "—"} • ${escapeHtml(car?.chargingStatus || "—")} • ${escapeHtml(car?.location || "—")}</div>
        </div>

        <div class="kpi-card">
          <div class="kpi-label"><span>☀️ Solaire & Conso Enphase</span><span>${energy.homeConsumptionW != null ? `Conso ${(energy.homeConsumptionW / 1000).toFixed(2)} kW` : "Prod. jour"}</span></div>
          <div class="kpi-value" style="color:${(energy.homeConsumptionW || 0) > 7000 && (energy.solarNowW || 0) <= 0 ? "var(--crit)" : "var(--warn)"}">${energy.solarNowW != null ? `${energy.solarNowW} W` : "—"}</div>
          <div class="kpi-sub">Maison : <strong>${energy.homeConsumptionW != null ? `${(energy.homeConsumptionW / 1000).toFixed(2)} kW` : "—"}</strong> • Prod. auj. : <strong>${escapeHtml(energy.solarTodayKwh || "—")} kWh</strong></div>
          <div class="kpi-sub">Prév. Auj: ${escapeHtml(energy.forecastTodayKwh || "—")} • Dem: ${escapeHtml(energy.forecastTomorrowKwh || "—")} kWh</div>
        </div>

        <div class="kpi-card">
          <div class="kpi-label"><span>🌡️ Confort & Piscine</span><span>${climate.length} zones</span></div>
          <div style="display:grid;grid-template-columns:repeat(2,1fr);gap:6px;margin-top:4px;">
            ${climate.slice(0, 4).map((c) => `
              <div style="background:var(--bg);padding:6px 8px;border-radius:8px;border:1px solid var(--border);">
                <div style="font-size:10px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${escapeHtml(c.name)}</div>
                <div style="font-size:16px;font-weight:800;">${escapeHtml(c.tempC)}°C ${c.humidityPct != null ? `<span style="font-size:11px;color:var(--muted);font-weight:400;">${c.humidityPct}%</span>` : ""}</div>
              </div>
            `).join("")}
          </div>
        </div>

        <div class="kpi-card">
          <div class="kpi-label"><span>💾 NAS JoNAS</span><span>${escapeHtml(nas?.status || "—")}</span></div>
          ${nas ? `
            <div class="kpi-value" style="color:${pctColor(nas.cpuPct || 0)}">CPU ${nas.cpuPct ?? "—"}%</div>
            <div class="kpi-sub">RAM : <strong>${nas.ramPct ?? "—"}%</strong> • Vol 1 : <strong>${escapeHtml(nas.volumeUsedPct)}%</strong></div>
            <div class="kpi-sub">Disques : ${nas.diskTempsC?.length ? nas.diskTempsC.map((t) => `${t}°C`).join(" / ") : "—"}</div>
          ` : `<div class="kpi-sub">Indisponible</div>`}
        </div>
      </div>

      <div class="grid-2" style="height:auto;">
        <div class="col">
          <div class="card">
            <div class="row" style="justify-content:space-between;margin-bottom:10px;">
              <h3>⚡ Prises & Serveurs (${totalPlugW} W total)</h3>
              <span class="badge">${plugs.length} capteurs</span>
            </div>
            ${plugs.length ? plugs.map((p) => {
              const pct = Math.min(100, Math.round((Math.max(0, p.powerW) / maxPlugW) * 100));
              return `
                <div class="plug-row">
                  <div style="width:160px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${escapeHtml(p.name)}</div>
                  <div class="bar-track" style="flex:1;margin-top:0;"><div class="bar-fill" style="width:${pct}%;background:var(--primary);"></div></div>
                  <div style="width:64px;text-align:right;font-weight:700;font-variant-numeric:tabular-nums;">${p.powerW} W</div>
                </div>
              `;
            }).join("") : "<span style='color:var(--muted)'>Aucune prise active détectée.</span>"}
          </div>
        </div>

        <div class="col">
          <div class="card">
            <h3>🏡 Volets, Robot & Équipements</h3>
            <div style="display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px;">
              <div class="metric-card">
                <div style="color:var(--muted);font-size:12px;">🪟 Volets ouverts</div>
                <div style="font-size:24px;font-weight:800;color:${(dev.coversOpen || 0) > 0 ? "var(--primary)" : "var(--muted)"}">${dev.coversOpen || 0} / ${dev.coversTotal || 0}</div>
                <div style="font-size:11px;color:var(--muted);margin-top:4px;">${dev.openCoverNames?.length ? dev.openCoverNames.map(escapeHtml).join(", ") : "Tous fermés"}</div>
              </div>
              <div class="metric-card">
                <div style="color:var(--muted);font-size:12px;">🤖 Aspirateur Rocky</div>
                <div style="font-size:22px;font-weight:800;">${escapeHtml(dev.vacuumState || "—")}</div>
                <div style="font-size:12px;color:var(--muted);margin-top:4px;">Batterie : ${dev.vacuumBatteryPct != null ? `${dev.vacuumBatteryPct}%` : "—"}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  `;

  document.getElementById("btn-refresh-ha").onclick = () => api("/api/org/refresh", "POST");
}

function renderStreamDeck(stage, snap) {
  const pc = snap.pcTelemetry;
  const buttons = snap.deckButtons || [];
  const cpu = pc ? pc.cpuPercent : 0;
  const ramPct = pc ? pc.ramPercent : 0;
  const ramUsedGb = pc ? (pc.ramUsedMb / 1024).toFixed(1) : "0.0";
  const ramTotalGb = pc ? (pc.ramTotalMb / 1024).toFixed(1) : "0.0";
  const gpuPct = pc ? (pc.gpuPercent || 0) : 0;
  const gpuName = pc?.gpuName || "GPU NVIDIA";
  const gpuMemPct = pc ? (pc.gpuMemPercent || 0) : 0;
  const gpuMemUsedGb = pc ? ((pc.gpuMemUsedMb || 0) / 1024).toFixed(1) : "0.0";
  const gpuMemTotalGb = pc ? ((pc.gpuMemTotalMb || 0) / 1024).toFixed(1) : "0.0";
  const limit = pc?.antigravityTokensLimit || 2000000;
  const used = pc?.antigravityTokensUsed || 0;
  const remaining = Math.max(0, limit - used);
  const quotaPct = Math.min(100, Math.round((remaining * 100) / limit));
  const agentOnline = pc && pc.updatedAtEpochMs && (Date.now() - pc.updatedAtEpochMs < 30000);

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <div>
          <h2>🎛️ Stream Deck & Télémétrie PC Temps Réel</h2>
          <div style="color:var(--muted);font-size:13px;">
            ${pc
              ? `<span style="color:${agentOnline ? "var(--ok)" : "var(--warn)"};font-weight:700;">● ${agentOnline ? "Agent PC Connecté" : "Agent PC en veille"}</span> • Poste : <strong>${escapeHtml(pc.hostname)}</strong> • Uptime ${Math.floor(pc.uptimeSecs / 3600)}h`
              : "⚠️ En attente de iris-pc-agent sur votre PC…"}
          </div>
        </div>
        <button class="btn" id="btn-toggle-deck-add">+ Ajouter une touche</button>
      </div>

      <div class="deck-telemetry">
        <div class="metric-card">
          <div class="row" style="justify-content:space-between;">
            <span style="color:var(--muted);font-size:12px;">⚡ CPU Processeur</span>
            <span class="badge">${pc ? escapeHtml(pc.hostname) : "PC"}</span>
          </div>
          <div style="font-size:30px;font-weight:800;margin-top:4px;color:${pctColor(cpu)};">${pc ? cpu + "%" : "—"}</div>
          <div style="font-size:12px;color:var(--muted);">Charge globale temps réel</div>
          <div class="bar-track"><div class="bar-fill" style="width:${cpu}%;background:${pctColor(cpu)};"></div></div>
        </div>

        <div class="metric-card">
          <div class="row" style="justify-content:space-between;">
            <span style="color:var(--muted);font-size:12px;">🧠 Mémoire RAM Système</span>
            <span style="font-size:12px;font-weight:700;color:var(--text);">${pc ? `${ramUsedGb} / ${ramTotalGb} Go` : "—"}</span>
          </div>
          <div style="font-size:30px;font-weight:800;margin-top:4px;color:${pctColor(ramPct)};">${pc ? ramPct + "%" : "—"}</div>
          <div style="font-size:12px;color:var(--muted);">${pc ? `${pc.ramUsedMb} Mo utilisés sur ${pc.ramTotalMb} Mo` : "Hors ligne"}</div>
          <div class="bar-track"><div class="bar-fill" style="width:${ramPct}%;background:${pctColor(ramPct)};"></div></div>
        </div>

        <div class="metric-card">
          <div class="row" style="justify-content:space-between;">
            <span style="color:var(--muted);font-size:12px;">🎮 GPU Charge</span>
            <span class="badge" style="background:rgba(34,197,94,0.16);color:#86efac;">${escapeHtml(gpuName)}</span>
          </div>
          <div style="font-size:30px;font-weight:800;margin-top:4px;color:${pctColor(gpuPct)};">${pc && pc.gpuMemTotalMb > 0 ? gpuPct + "%" : "—"}</div>
          <div style="font-size:12px;color:var(--muted);">Utilisation cœur graphique</div>
          <div class="bar-track"><div class="bar-fill" style="width:${gpuPct}%;background:${pctColor(gpuPct)};"></div></div>
        </div>

        <div class="metric-card">
          <div class="row" style="justify-content:space-between;">
            <span style="color:var(--muted);font-size:12px;">📼 Mémoire VRAM GPU</span>
            <span style="font-size:12px;font-weight:700;color:var(--text);">${pc && pc.gpuMemTotalMb > 0 ? `${gpuMemUsedGb} / ${gpuMemTotalGb} Go` : "—"}</span>
          </div>
          <div style="font-size:30px;font-weight:800;margin-top:4px;color:${pctColor(gpuMemPct)};">${pc && pc.gpuMemTotalMb > 0 ? gpuMemPct + "%" : "—"}</div>
          <div style="font-size:12px;color:var(--muted);">${pc && pc.gpuMemTotalMb > 0 ? `${pc.gpuMemUsedMb} Mo / ${pc.gpuMemTotalMb} Mo dédiés` : "En attente nvidia-smi"}</div>
          <div class="bar-track"><div class="bar-fill" style="width:${gpuMemPct}%;background:${pctColor(gpuMemPct)};"></div></div>
        </div>

        <div class="metric-card">
          <div style="color:var(--muted);font-size:12px;">🤖 Quota Antigravity restant</div>
          <div style="font-size:26px;font-weight:800;margin-top:4px;">${(remaining / 1000).toFixed(0)}k <span style="font-size:13px;color:var(--muted);">/ ${(limit / 1000).toFixed(0)}k</span></div>
          <div style="font-size:12px;color:var(--primary);">${escapeHtml(pc?.antigravityModel || "Gemini 4 Hydrogen")}</div>
          <div class="bar-track"><div class="bar-fill" style="width:${quotaPct}%;background:var(--primary);"></div></div>
        </div>

        <div class="metric-card">
          <div style="color:var(--muted);font-size:12px;">📡 Interaction PC & Réunions</div>
          <div style="font-size:20px;font-weight:800;margin-top:6px;">${pc?.activeMeeting ? "🟢 En réunion " + escapeHtml(pc.activeMeeting) : "⚪ Aucune réunion active"}</div>
          <div style="font-size:12px;color:var(--primary);margin-top:6px;font-weight:600;" id="deck-last-action-label">
            ${snap.lastDeckAction ? `⚡ Exécuté : ${escapeHtml(snap.lastDeckAction.label)}` : "Prêt à exécuter sur le PC"}
          </div>
        </div>
      </div>

      <div id="deck-add-form" class="card" style="display:none;margin-bottom:16px;">
        <div class="row">
          <input id="dk-label" placeholder="Nom (ex: Update UniGetUI)" style="flex:1;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
          <input id="dk-icon" placeholder="Icône (📦)" style="width:90px;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
          <select id="dk-kind" style="background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;">
            <option value="JOIN_TEAMS">Rejoindre Teams</option>
            <option value="JOIN_ZOOM">Rejoindre Zoom</option>
            <option value="OPEN_URL">Ouvrir URL</option>
            <option value="LAUNCH_APP">Lancer App</option>
            <option value="MEDIA_TOGGLE">Média Play/Pause</option>
            <option value="LOCK_SCREEN">Verrouiller PC</option>
            <option value="UPDATE_UNI_GET">Update UniGetUI / Winget</option>
            <option value="UPDATE_PROXMOX_VMS">Update VMs Proxmox</option>
            <option value="SWITCH_IRIS_SCREEN">Changer Écran Iris</option>
          </select>
          <input id="dk-payload" placeholder="URL / Commande / Screen ID" style="flex:2;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
          <button class="btn primary" id="btn-save-deck">Ajouter</button>
        </div>
      </div>

      <div class="deck-grid" id="deck-grid"></div>
    </div>
  `;

  document.getElementById("btn-toggle-deck-add").onclick = () => {
    const form = document.getElementById("deck-add-form");
    form.style.display = form.style.display === "none" ? "block" : "none";
  };
  document.getElementById("btn-save-deck").onclick = () => {
    const label = document.getElementById("dk-label").value.trim();
    const icon = document.getElementById("dk-icon").value.trim() || "⚡";
    const kind = document.getElementById("dk-kind").value;
    const payload = document.getElementById("dk-payload").value.trim();
    if (!label) return;
    api("/api/deck/buttons", "POST", {
      id: `deck-${Date.now()}`,
      label,
      subtitle: kind,
      icon,
      color: "#38bdf8",
      category: "Custom",
      kind,
      payload
    });
  };

  const grid = document.getElementById("deck-grid");
  buttons.forEach((btn) => {
    const el = document.createElement("button");
    el.className = "deck-tile";
    el.style.borderTop = `3px solid ${btn.color || "#38bdf8"}`;
    el.innerHTML = `
      <div class="row" style="justify-content:space-between;width:100%;">
        <span class="deck-icon">${escapeHtml(btn.icon || "⚡")}</span>
        <span class="deck-cat">${escapeHtml(btn.category || "")}</span>
      </div>
      <div>
        <div style="font-weight:700;font-size:15px;">${escapeHtml(btn.label)}</div>
        <div style="font-size:12px;color:var(--muted);margin-top:2px;" class="deck-sub">${escapeHtml(btn.subtitle || "")}</div>
      </div>
    `;
    el.onclick = async () => {
      const sub = el.querySelector(".deck-sub");
      const prevText = sub ? sub.textContent : "";
      el.style.borderColor = "var(--ok)";
      if (sub) sub.textContent = "⚡ Commande envoyée…";
      await api("/api/deck/trigger", "POST", { buttonId: btn.id });
      setTimeout(() => {
        el.style.borderColor = "";
        if (sub) sub.textContent = prevText;
      }, 1500);
    };
    grid.appendChild(el);
  });
}

function renderWeatherCard(weather) {
  if (!weather) {
    return `<div class="card"><h3>Météo Lyon (3 jours)</h3><p style="color:var(--muted);margin-top:8px;">Chargement en cours…</p></div>`;
  }
  const daysHtml = (weather.days || []).map((d, idx) => {
    const dayLabel = idx === 0 ? "Aujourd'hui" : idx === 1 ? "Demain" : formatShortDate(d.date);
    return `
      <div style="flex:1;background:var(--surface2);border:1px solid var(--border);border-radius:12px;padding:12px;text-align:center;">
        <div style="font-size:12px;color:var(--muted);">${escapeHtml(dayLabel)}</div>
        <div style="font-size:28px;margin:4px 0;">${d.icon}</div>
        <div style="font-weight:700;">${d.tempMinC}° / ${d.tempMaxC}°C</div>
        <div style="font-size:12px;color:var(--primary);">${escapeHtml(d.label)}</div>
        <div style="font-size:11px;color:var(--muted);margin-top:4px;">💧 ${d.precipProbPct}% · 💨 ${d.windMaxKmh} km/h</div>
      </div>
    `;
  }).join("");

  return `
    <div class="card">
      <div class="row" style="justify-content:space-between;margin-bottom:10px;">
        <h3>${weather.currentIcon} Météo ${escapeHtml(weather.city)} — ${weather.currentTempC}°C</h3>
        <span style="font-size:12px;color:var(--primary);">${escapeHtml(weather.currentLabel)} • 3 jours</span>
      </div>
      <div class="row" style="align-items:stretch;">${daysHtml}</div>
    </div>
  `;
}

function formatShortDate(iso) {
  try {
    const [y, m, d] = iso.split("-").map(Number);
    const dt = new Date(y, m - 1, d);
    return dt.toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" });
  } catch (_) {
    return iso;
  }
}

function getEasterDate(year) {
  const a = year % 19;
  const b = Math.floor(year / 100);
  const c = year % 100;
  const d = Math.floor(b / 4);
  const e = b % 4;
  const f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3);
  const h = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4);
  const k = c % 4;
  const l = (32 + 2 * e + 2 * i - h - k) % 7;
  const m = Math.floor((a + 11 * h + 22 * l) / 451);
  const month = Math.floor((h + l - 7 * m + 114) / 31);
  const day = ((h + l - 7 * m + 114) % 31) + 1;
  return new Date(year, month - 1, day);
}

function getFrenchHolidayName(dateObj) {
  const y = dateObj.getFullYear();
  const mmdd = `${String(dateObj.getMonth() + 1).padStart(2, "0")}-${String(dateObj.getDate()).padStart(2, "0")}`;
  const fixed = {
    "01-01": "Jour de l'an",
    "05-01": "Fête du Travail",
    "05-08": "Victoire 1945",
    "07-14": "Fête nationale",
    "08-15": "Assomption",
    "11-01": "Toussaint",
    "11-11": "Armistice 1918",
    "12-25": "Noël"
  };
  if (fixed[mmdd]) return fixed[mmdd];

  const easter = getEasterDate(y);
  const addDaysIso = (base, offset) => toLocalIsoDate(new Date(base.getFullYear(), base.getMonth(), base.getDate() + offset));
  const targetIso = toLocalIsoDate(dateObj);
  if (targetIso === addDaysIso(easter, 1)) return "Lundi de Pâques";
  if (targetIso === addDaysIso(easter, 39)) return "Ascension";
  if (targetIso === addDaysIso(easter, 50)) return "Lundi de Pentecôte";
  return null;
}

function getWeekDays(count = 7) {
  const now = new Date();
  const todayIso = toLocalIsoDate(now);
  const days = [];
  for (let i = 0; i < count; i++) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() + i);
    const iso = toLocalIsoDate(d);
    const dow = d.getDay(); // 0=Dim..6=Sam
    const weekday = d.toLocaleDateString("fr-FR", { weekday: "short" }).replace(".", "");
    const monthShort = d.toLocaleDateString("fr-FR", { month: "short" }).replace(".", "");
    const holidayName = getFrenchHolidayName(d);
    days.push({
      iso,
      dow,
      dayNum: d.getDate(),
      monthShort,
      weekday: weekday.charAt(0).toUpperCase() + weekday.slice(1),
      isToday: iso === todayIso,
      isWeekend: dow === 0 || dow === 6,
      holidayName,
      isHoliday: Boolean(holidayName)
    });
  }
  return days;
}

function getRecurringFamilyEvents(days) {
  const out = [];
  days.forEach((d) => {
    if (d.dow === 1 || d.dow === 3) {
      out.push({
        id: `rec-capmo-${d.iso}`,
        title: "🏢 Capmo",
        date: d.iso,
        startTime: "09:00",
        endTime: "",
        location: "Capmo",
        source: "recurring"
      });
    }
    if (d.dow === 3) {
      out.push({
        id: `rec-alice-theatre-${d.iso}`,
        title: "🎭 Alice — Théâtre",
        date: d.iso,
        startTime: "14:00",
        endTime: "",
        location: "Après-midi",
        source: "recurring"
      });
    }
    if (d.dow === 5) {
      out.push({
        id: `rec-victor-handisport-${d.iso}`,
        title: "🏅 Victor — Handisport",
        date: d.iso,
        startTime: "19:30",
        endTime: "",
        location: "Handisport",
        source: "recurring"
      });
    }
  });
  return out;
}

function getRollingEvents(snap, days) {
  const isoSet = new Set(days.map((d) => d.iso));
  const base = (snap.org?.events || []).filter((e) => isoSet.has(e.date));
  const recurring = getRecurringFamilyEvents(days);
  return [...base, ...recurring].sort((a, b) => {
    const cmpDate = (a.date || "").localeCompare(b.date || "");
    if (cmpDate !== 0) return cmpDate;
    return (a.startTime || "99:99").localeCompare(b.startTime || "99:99");
  });
}

function getEdDayBounds(dayEvents) {
  const byChild = new Map();
  dayEvents.forEach((e) => {
    if (e.source !== "ecoledirecte") return;
    if (!e.startTime || (e.title || "").startsWith("[ANNULÉ]")) return;
    const match = /^\[([^\]]+)\]\s*/.exec(e.title || "");
    const child = match ? match[1] : "Enfants";
    const end = e.endTime || e.startTime;
    const prev = byChild.get(child);
    if (!prev) {
      byChild.set(child, { child, startTime: e.startTime, endTime: end, count: 1 });
    } else {
      if (e.startTime < prev.startTime) prev.startTime = e.startTime;
      if (end > prev.endTime) prev.endTime = end;
      prev.count += 1;
    }
  });
  return Array.from(byChild.values());
}

function toLocalIsoDate(d) {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

function renderOrgOverview(stage, snap) {
  const now = new Date();
  const timeStr = now.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  const dateStr = now.toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  const notes = snap.notes.filter((n) => !n.done).slice(0, 4);
  const mails = (snap.org?.mails || []).slice(0, 4);
  const rollingDays = getWeekDays(7);
  const allRollingEvents = getRollingEvents(snap, rollingDays);
  const events = allRollingEvents.slice(0, 5);
  const comms = snap.comms || {};
  const shopPending = (snap.shoppingList || []).filter((i) => !i.completed).length;
  const driveReady = (snap.driveOrders || []).filter((o) => o.readyForPickup).length;
  const cluster = snap.infra?.cluster;
  const offlineCount = cluster?.offlineNodes?.length || 0;
  const openPrs = snap.infra?.openPrs || [];
  const kusanagi = snap.infra?.kusanagi;
  const ha = snap.homeAssistant;
  const homeCount = (ha?.people || []).filter((p) => p.isHome).length;
  const argo = kusanagi?.argocd;
  const kAlerts = kusanagi?.alerts;

  stage.innerHTML = `
    <div class="org-view">
      <div class="grid-2">
        <div class="col">
          <div class="card">
            <div class="clock-big" id="hub-clock">${timeStr}</div>
            <div class="date-sub">${dateStr}</div>
          </div>
          ${renderWeatherCard(snap.org?.weather)}
          <div class="card">
            <h3>Pilotage MQTT & Incidents</h3>
            <p style="margin-top:8px;">MQTT : <strong>${snap.mqttState}</strong> • ${snap.activeAlert ? "⚠️ " + escapeHtml(snap.activeAlert.title) : "✅ Nominal"}</p>
            <p style="color:var(--muted); font-size:12px;">Topic : ${snap.settings.mqtt.baseTopic}/${snap.settings.mqtt.deviceId}/command/#</p>
          </div>
          <div class="card">
            <h3>🔔 Synthèse Vie & Infra</h3>
            <div style="display:grid;grid-template-columns:repeat(2,1fr);gap:8px;margin-top:10px;font-size:13px;">
              <div>📞 Appels manqués : <strong style="color:${(comms.missedCalls || 0) > 0 ? "var(--crit)" : "inherit"}">${comms.missedCalls || 0}</strong></div>
              <div>📼 Répondeur : <strong style="color:${(comms.voicemails || 0) > 0 ? "var(--warn)" : "inherit"}">${comms.voicemails || 0}</strong></div>
              <div>💬 Teams non lus : <strong>${comms.teamsUnread || 0}</strong></div>
              <div>💜 Slack en suspens : <strong style="color:${(comms.slackUnread || 0) > 0 ? "var(--primary)" : "inherit"}">${comms.slackUnread || (comms.slackMessages?.length || 0)}</strong></div>
              <div>✉️ Mails non lus : <strong>${comms.mailsUnread || 0}</strong></div>
              <div>🛒 Courses Alexa : <strong>${shopPending}</strong></div>
              <div>🚗 ClearDrive prêt : <strong style="color:${driveReady > 0 ? "var(--ok)" : "inherit"}">${driveReady}</strong></div>
              <div>☸️ Cluster jo3 : <strong style="color:${offlineCount > 0 ? "var(--crit)" : "var(--ok)"}">${cluster ? `${cluster.nodesReady}/${cluster.nodesTotal} Ready` : "—"}</strong></div>
              <div>🔀 PRs GitHub : <strong>${openPrs.length}</strong></div>
              <div>🚙 Volvo EX40 : <strong>${ha?.vehicle?.batteryPct != null ? `${ha.vehicle.batteryPct}% (${ha.vehicle.rangeKm ?? "—"} km)` : "—"}</strong></div>
              <div>🏡 Présence / Énergie : <strong>${ha?.connected ? `${homeCount} à la maison • ${ha.energy?.homeConsumptionW != null ? `${(ha.energy.homeConsumptionW / 1000).toFixed(1)} kW` : "—"} (☀️ ${ha.energy?.solarNowW ?? 0} W)` : "—"}</strong></div>
              <div>🚀 ArgoCD : <strong style="color:${(argo?.degraded || 0) > 0 ? "var(--crit)" : "var(--ok)"}">${argo ? `${argo.healthy}/${argo.total} Healthy` : "—"}</strong></div>
              <div>🚨 Alertes K8s : <strong style="color:${(kAlerts?.criticalCount || 0) > 0 ? "var(--crit)" : "var(--ok)"}">${kAlerts ? `${kAlerts.criticalCount} crit / ${kAlerts.warningCount} warn` : "—"}</strong></div>
            </div>
          </div>
        </div>
        <div class="col">
          <div class="launchers" id="hub-launchers"></div>
          <div class="card">
            <h3>📧 Mails du jour (${mails.length})</h3>
            <div style="margin-top:8px;">
              ${mails.length ? mails.map((m) => `<div style="margin-bottom:6px;"><strong>${escapeHtml(m.sender)}</strong> — ${escapeHtml(m.subject)} <span style="color:var(--muted);font-size:12px;">${escapeHtml(m.receivedAt)}</span></div>`).join("") : "<span style='color:var(--muted)'>Aucun mail du jour.</span>"}
            </div>
          </div>
          <div class="card">
            <div class="row" style="justify-content:space-between;align-items:center;">
              <h3>📅 7 prochains jours (${allRollingEvents.length})</h3>
              <button class="btn" id="btn-hub-open-agenda" style="padding:4px 10px;font-size:12px;">Voir / Ajouter →</button>
            </div>
            <div class="week-strip">
              ${rollingDays.map((d) => {
                const dayEvts = allRollingEvents.filter((e) => e.date === d.iso);
                const edBounds = getEdDayBounds(dayEvts);
                const pillClass = [
                  "week-pill",
                  d.isToday ? "today" : "",
                  d.isWeekend ? "weekend" : "",
                  d.isHoliday ? "holiday" : ""
                ].filter(Boolean).join(" ");
                const headerColor = d.isToday
                  ? "var(--primary)"
                  : d.isHoliday
                    ? "#fcd34d"
                    : d.isWeekend
                      ? "#c4b5fd"
                      : "var(--muted)";
                return `
                  <div class="${pillClass}" title="${escapeHtml(d.holidayName || "")}">
                    <div style="color:${headerColor};font-weight:700;">${d.weekday}</div>
                    <div style="font-size:14px;font-weight:800;margin:2px 0;">${d.dayNum}</div>
                    ${d.isHoliday ? `<div style="font-size:10px;color:#fcd34d;font-weight:700;">🇫🇷 Férié</div>` : ""}
                    ${edBounds.length ? edBounds.map((b) => `<div style="font-size:10px;color:#e9d5ff;font-weight:600;">${escapeHtml(b.startTime)}→${escapeHtml(b.endTime)}</div>`).join("") : `<div style="color:${dayEvts.length > 0 ? "var(--ok)" : "var(--muted)"};font-weight:600;">${dayEvts.length > 0 ? `${dayEvts.length} evt` : "Libre"}</div>`}
                  </div>
                `;
              }).join("")}
            </div>
            <div style="margin-top:10px;">
              ${events.length ? events.map((e) => `<div style="margin-bottom:6px;"><span style="color:var(--primary);font-weight:600;">${escapeHtml(formatShortDate(e.date))} ${escapeHtml(e.startTime)}</span> — ${escapeHtml(e.title)}</div>`).join("") : "<span style='color:var(--muted);font-size:13px;'>Aucun événement prévu sur les 7 prochains jours.</span>"}
            </div>
          </div>
          <div class="card">
            <h3>📌 Notes prioritaires</h3>
            <div style="margin-top:8px;">
              ${notes.length ? notes.map((n) => `<div style="margin-bottom:4px;">${n.pinned ? "📌 " : "• "}${escapeHtml(n.text)}</div>`).join("") : "<span style='color:var(--muted)'>Aucune tâche en attente.</span>"}
            </div>
          </div>
          ${renderRecentAgentReportsCard(snap, "btn-hub-open-agents", true)}
          ${(kusanagi?.news?.length) ? `
          <div class="card">
            <div class="row" style="justify-content:space-between;align-items:center;">
              <h3>📰 À la une (Kusanagi)</h3>
              <button class="btn" id="btn-hub-open-news" style="padding:4px 10px;font-size:12px;">Tout voir →</button>
            </div>
            <div style="margin-top:8px;">
              ${kusanagi.news.slice(0, 3).map((n) => `
                <div style="margin-bottom:6px;font-size:13px;">
                  <span class="badge" style="margin-right:6px;">${escapeHtml(n.icon || "🗞️")} ${escapeHtml(n.source)}</span>
                  <a href="${escapeHtml(n.url)}" target="_blank" rel="noopener" style="color:var(--text);text-decoration:none;font-weight:600;">${escapeHtml(n.title)}</a>
                </div>
              `).join("")}
            </div>
          </div>
          ` : ""}
        </div>
      </div>
    </div>
  `;
  const btnAgentsHub = document.getElementById("btn-hub-open-agents");
  if (btnAgentsHub) {
    btnAgentsHub.onclick = () => api("/api/screen/select", "POST", { target: "org-agents" });
  }
  const btnAgenda = document.getElementById("btn-hub-open-agenda");
  if (btnAgenda) {
    btnAgenda.onclick = () => api("/api/screen/select", "POST", { target: "org-agenda" });
  }
  const btnNews = document.getElementById("btn-hub-open-news");
  if (btnNews) {
    btnNews.onclick = () => api("/api/screen/select", "POST", { target: "org-news" });
  }
  const launchers = document.getElementById("hub-launchers");
  snap.screens.filter((s) => s.enabled).forEach((s) => {
    const btn = document.createElement("button");
    btn.className = "launcher";
    btn.innerHTML = `<strong>${escapeHtml(s.name)}</strong><div style="color:var(--muted);font-size:12px;margin-top:4px;">${s.type}</div>`;
    btn.onclick = () => api("/api/screen/select", "POST", { target: s.id });
    launchers.appendChild(btn);
  });
}

function renderLifeInfra(stage, snap) {
  const comms = snap.comms || {};
  const missedDetails = comms.missedCallDetails || [];
  const voicemailDetails = comms.voicemailDetails || [];
  const shopping = snap.shoppingList || [];
  const driveOrders = snap.driveOrders || [];
  const cluster = snap.infra?.cluster;
  const offlineNodes = cluster?.offlineNodes || [];
  const openPrs = snap.infra?.openPrs || [];
  const kusanagi = snap.infra?.kusanagi;
  const kCluster = kusanagi?.cluster;
  const proxmoxNodes = kusanagi?.proxmoxNodes || [];
  const proxmoxVms = kusanagi?.proxmoxVms || [];
  const slackMessages = comms.slackMessages || [];
  const argo = kusanagi?.argocd;
  const kAlerts = kusanagi?.alerts;

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <div>
          <h2>Vie Quotidienne, Kusanagi & Infra jo3</h2>
          <div style="color:var(--muted);font-size:13px;">Capteurs Kusanagi (Kubernetes, Proxmox, ArgoCD, Alertes), Téléphone, Alexa & ClearDrive</div>
        </div>
        <button class="btn primary" id="btn-refresh-infra">↻ Actualiser Infra & Capteurs</button>
      </div>

      <div class="kpi-grid-4">
        <div class="kpi-card">
          <div class="kpi-label"><span>☸️ Cluster jo3</span><span>${kCluster ? `${kCluster.servicesTotal} svcs` : "—"}</span></div>
          <div class="kpi-value" style="color:${offlineNodes.length > 0 ? "var(--crit)" : "var(--ok)"}">${kCluster ? `${kCluster.nodesReady}/${kCluster.nodesTotal}` : (cluster ? `${cluster.nodesReady}/${cluster.nodesTotal}` : "—")}</div>
          <div class="kpi-sub">${kCluster ? `${kCluster.podsRunning} / ${kCluster.podsTotal} pods Running` : "Nœuds Ready"}</div>
        </div>

        <div class="kpi-card">
          <div class="kpi-label"><span>🚀 ArgoCD GitOps</span><span>${argo ? `${argo.synced}/${argo.total} synced` : "—"}</span></div>
          <div class="kpi-value" style="color:${(argo?.degraded || 0) > 0 ? "var(--crit)" : "var(--ok)"}">${argo ? `${argo.healthy}/${argo.total}` : "—"}</div>
          <div class="kpi-sub">${argo ? `${argo.degraded} dégradée(s) • ${argo.outOfSync} out-of-sync` : "En attente Kusanagi"}</div>
        </div>

        <div class="kpi-card">
          <div class="kpi-label"><span>🚨 Alertmanager</span><span>Cluster</span></div>
          <div class="kpi-value" style="color:${(kAlerts?.criticalCount || 0) > 0 ? "var(--crit)" : (kAlerts?.warningCount || 0) > 0 ? "var(--warn)" : "var(--ok)"}">
            ${kAlerts ? `${kAlerts.criticalCount} crit` : "—"}
          </div>
          <div class="kpi-sub">${kAlerts ? `${kAlerts.warningCount} warning(s) actives` : "En attente Kusanagi"}</div>
        </div>

        <div class="kpi-card">
          <div class="kpi-label"><span>🔀 PRs GitHub</span><span>Dépôts suivis</span></div>
          <div class="kpi-value" style="color:var(--primary)">${openPrs.length}</div>
          <div class="kpi-sub">${openPrs.filter((p) => p.isDependencyUpdate).length} mise(s) à jour dépendance</div>
        </div>
      </div>

      ${proxmoxNodes.length ? `
        <div class="card" style="margin-bottom:16px;">
          <div class="row" style="justify-content:space-between;margin-bottom:10px;">
            <div class="row" style="gap:8px;">
              <h3>🖥️ Hyperviseurs & VMs Proxmox (Kusanagi)</h3>
              <span class="badge">${proxmoxNodes.length} nœuds • ${proxmoxVms.length} VMs/LXC</span>
            </div>
            <button class="btn" id="btn-proxmox-update" style="padding:5px 12px;font-size:12px;">🔄 Update APT Nodes & VMs</button>
          </div>
          <div class="proxmox-grid">
            ${proxmoxNodes.map((n) => `
              <div class="node-card">
                <div class="row" style="justify-content:space-between;margin-bottom:6px;">
                  <strong>${escapeHtml(n.node)}</strong>
                  <span style="font-size:11px;color:${n.status === "online" ? "var(--ok)" : "var(--crit)"};font-weight:700;">● ${escapeHtml(n.status)} • up ${formatUptime(n.uptimeSecs)}</span>
                </div>
                <div class="mini-bar-row">
                  <span style="width:42px;color:var(--muted);">CPU</span>
                  <div class="bar-track"><div class="bar-fill" style="width:${n.cpuPct}%;background:${pctColor(n.cpuPct)};"></div></div>
                  <span style="width:44px;text-align:right;font-weight:700;">${n.cpuPct}%</span>
                </div>
                <div class="mini-bar-row">
                  <span style="width:42px;color:var(--muted);">RAM</span>
                  <div class="bar-track"><div class="bar-fill" style="width:${n.memPct}%;background:${pctColor(n.memPct)};"></div></div>
                  <span style="width:88px;text-align:right;">${escapeHtml(n.memUsedGb)}/${escapeHtml(n.memTotalGb)}G</span>
                </div>
                <div class="mini-bar-row">
                  <span style="width:42px;color:var(--muted);">Disk</span>
                  <div class="bar-track"><div class="bar-fill" style="width:${n.diskPct}%;background:${pctColor(n.diskPct)};"></div></div>
                  <span style="width:88px;text-align:right;">${escapeHtml(n.diskUsedGb)}/${escapeHtml(n.diskTotalGb)}G</span>
                </div>
              </div>
            `).join("")}
          </div>
          ${proxmoxVms.length ? `
            <div style="margin-top:12px;display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:8px;">
              ${proxmoxVms.map((v) => `
                <div style="background:var(--bg);border:1px solid var(--border);border-radius:10px;padding:8px 10px;font-size:12px;">
                  <div class="row" style="justify-content:space-between;">
                    <strong>#${v.vmid} ${escapeHtml(v.name)}</strong>
                    <span style="color:${v.status === "running" ? "var(--ok)" : "var(--muted)"};font-weight:700;">● ${escapeHtml(v.status)}</span>
                  </div>
                  <div style="color:var(--muted);margin-top:4px;">${escapeHtml(v.node)} (${escapeHtml(v.kind)}) • CPU ${v.cpuPct}% • RAM ${escapeHtml(v.memUsedGb)}/${escapeHtml(v.memTotalGb)}G</div>
                </div>
              `).join("")}
            </div>
          ` : ""}
        </div>
      ` : ""}

      ${kAlerts?.topAlerts?.length ? `
        <div class="card" style="margin-bottom:16px;border-color:rgba(239,68,68,0.45);">
          <h3 style="margin-bottom:8px;">🚨 Alertes actives Prometheus / Alertmanager</h3>
          ${kAlerts.topAlerts.map((a) => `
            <div style="padding:6px 0;border-bottom:1px solid var(--border);font-size:13px;">
              <span class="badge" style="background:${a.severity === "critical" ? "rgba(239,68,68,0.22)" : "rgba(245,158,11,0.22)"};color:${a.severity === "critical" ? "#fecaca" : "#fde68a"};">${escapeHtml(a.severity)}</span>
              <strong style="margin-left:6px;">${escapeHtml(a.name)}</strong>
              <span style="color:var(--muted);">(${escapeHtml(a.namespace)})</span>
              ${a.summary ? `<span style="color:var(--muted);margin-left:6px;">— ${escapeHtml(a.summary)}</span>` : ""}
            </div>
          `).join("")}
        </div>
      ` : ""}

      <div class="grid-2">
        <div class="col">
          <div class="card">
            <div class="row" style="justify-content:space-between;align-items:center;">
              <h3>📞 Téléphone & Messagerie</h3>
              <button class="btn" id="btn-ack-comms" style="padding:4px 10px;font-size:12px;">✓ Acquitter</button>
            </div>
            <div style="display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px;">
              <div class="metric-card">
                <div style="color:var(--muted);font-size:12px;">Appels en absence</div>
                <div style="font-size:26px;font-weight:800;color:${(comms.missedCalls || 0) > 0 ? "var(--crit)" : "var(--ok)"}">${comms.missedCalls || 0}</div>
              </div>
              <div class="metric-card">
                <div style="color:var(--muted);font-size:12px;">Messages répondeur</div>
                <div style="font-size:26px;font-weight:800;color:${(comms.voicemails || 0) > 0 ? "var(--warn)" : "var(--ok)"}">${comms.voicemails || 0}</div>
              </div>
              <div class="metric-card">
                <div style="color:var(--muted);font-size:12px;">Teams / Slack</div>
                <div style="font-size:24px;font-weight:800;">${comms.teamsUnread || 0} <span style="font-size:14px;color:var(--primary);">/ ${comms.slackUnread || slackMessages.length} Slack</span></div>
              </div>
              <div class="metric-card">
                <div style="color:var(--muted);font-size:12px;">Mails non lus</div>
                <div style="font-size:26px;font-weight:800;">${comms.mailsUnread || 0}</div>
              </div>
            </div>
            ${missedDetails.length ? `<div style="margin-top:10px;font-size:13px;"><strong>Appels manqués :</strong> ${missedDetails.map((d) => escapeHtml(d)).join(" • ")}</div>` : ""}
            ${voicemailDetails.length ? `<div style="margin-top:6px;font-size:13px;"><strong>Répondeur :</strong> ${voicemailDetails.map((d) => escapeHtml(d)).join(" • ")}</div>` : ""}
            ${slackMessages.length ? `
              <div style="margin-top:12px;border-top:1px solid var(--border);padding-top:10px;">
                <div class="row" style="justify-content:space-between;margin-bottom:6px;">
                  <strong style="font-size:13px;">💜 Messages Slack en suspens (${slackMessages.length})</strong>
                  <span class="badge">${escapeHtml(slackMessages[0]?.channel || "#ai")}</span>
                </div>
                <div style="max-height:200px;overflow-y:auto;display:flex;flex-direction:column;gap:6px;">
                  ${slackMessages.map((m) => `
                    <div style="background:var(--bg);border:1px solid var(--border);border-radius:8px;padding:7px 10px;font-size:12px;">
                      <div class="row" style="justify-content:space-between;">
                        <strong style="color:var(--primary);">${escapeHtml(m.sender)}</strong>
                        <span style="color:var(--muted);font-size:11px;">${escapeHtml(m.tsLabel)}</span>
                      </div>
                      <div style="margin-top:3px;line-height:1.35;word-break:break-word;">${escapeHtml(m.text.length > 180 ? m.text.slice(0, 180) + "…" : m.text)}</div>
                    </div>
                  `).join("")}
                </div>
              </div>
            ` : ""}
          </div>

          <div class="card">
            <h3>🛒 Liste d'achats Alexa (${shopping.filter((i) => !i.completed).length} à acheter)</h3>
            <div class="row" style="margin-top:10px;margin-bottom:10px;">
              <input id="shop-new-name" placeholder="Ajouter un article Alexa..." style="flex:1;" />
              <button class="btn primary" id="btn-add-shop">+ Ajouter</button>
            </div>
            <div id="shop-list">
              ${shopping.length ? shopping.map((item) => `
                <div class="row" style="justify-content:space-between;padding:6px 0;border-bottom:1px solid var(--border);">
                  <label style="display:flex;align-items:center;gap:8px;cursor:pointer;text-decoration:${item.completed ? "line-through" : "none"};opacity:${item.completed ? "0.55" : "1"};">
                    <input type="checkbox" class="shop-toggle" data-id="${escapeHtml(item.id)}" ${item.completed ? "checked" : ""} />
                    <span>${escapeHtml(item.name)}</span>
                  </label>
                  <button class="btn shop-del" data-id="${escapeHtml(item.id)}" style="padding:4px 8px;">✕</button>
                </div>
              `).join("") : "<span style='color:var(--muted)'>Liste d'achats Alexa vide.</span>"}
            </div>
          </div>
        </div>

        <div class="col">
          <div class="card">
            <h3>🚗 ClearDrive — Commandes à relever (${driveOrders.filter((o) => o.readyForPickup).length} prête(s))</h3>
            <div class="row" style="margin-top:10px;margin-bottom:10px;">
              <input id="drive-new-store" placeholder="Magasin (ex: ClearDrive)" value="ClearDrive" style="flex:1;" />
              <input id="drive-new-slot" placeholder="Créneau (ex: 17h30)" style="flex:1;" />
              <button class="btn primary" id="btn-add-drive">+ Commande</button>
            </div>
            <div>
              ${driveOrders.length ? driveOrders.map((o) => `
                <div class="row" style="justify-content:space-between;padding:8px 0;border-bottom:1px solid var(--border);">
                  <div>
                    <strong>${escapeHtml(o.store)}</strong> — <span>${escapeHtml(o.status)}</span>
                    ${o.readyForPickup ? `<span class="badge" style="background:var(--ok);color:#000;margin-left:6px;">À relever</span>` : ""}
                    ${o.pickupSlot ? `<div style="color:var(--muted);font-size:12px;">Créneau : ${escapeHtml(o.pickupSlot)}</div>` : ""}
                  </div>
                  <button class="btn drive-del" data-id="${escapeHtml(o.id)}" style="padding:4px 8px;">✕</button>
                </div>
              `).join("") : "<span style='color:var(--muted)'>Aucune commande ClearDrive en cours.</span>"}
            </div>
          </div>

          <div class="card">
            <h3>☸️ Cluster Kubernetes jo3</h3>
            <div style="margin-top:8px;">
              ${cluster ? `
                <div style="font-size:15px;font-weight:700;color:${offlineNodes.length > 0 ? "var(--crit)" : "var(--ok)"}">
                  ${offlineNodes.length > 0 ? `⚠️ ${offlineNodes.length} node(s) offline` : "✅ Tous les nodes sont Ready"}
                  (${cluster.nodesReady} / ${cluster.nodesTotal})
                </div>
                ${offlineNodes.length > 0 ? `<div style="margin-top:6px;color:var(--crit);font-size:13px;">Offline : ${offlineNodes.map((n) => escapeHtml(n)).join(", ")}</div>` : ""}
              ` : "<span style='color:var(--muted)'>Statut cluster jo3 non encore remonté (en attente de l'API Kubernetes).</span>"}
            </div>
          </div>

          <div class="card">
            <h3>🔀 PRs GitHub & Mises à jour (${openPrs.length})</h3>
            <div style="margin-top:8px;max-height:260px;overflow-y:auto;">
              ${openPrs.length ? openPrs.map((pr) => `
                <div style="padding:6px 0;border-bottom:1px solid var(--border);font-size:13px;">
                  <div>
                    <a href="${escapeHtml(pr.url)}" target="_blank" rel="noopener" style="color:var(--primary);font-weight:600;text-decoration:none;">
                      ${escapeHtml(pr.repo)} #${pr.number}
                    </a>
                    ${pr.isDependencyUpdate ? `<span class="badge" style="margin-left:6px;">Dépendance</span>` : ""}
                  </div>
                  <div>${escapeHtml(pr.title)}</div>
                  <div style="color:var(--muted);font-size:11px;">par ${escapeHtml(pr.author)}</div>
                </div>
              `).join("") : "<span style='color:var(--muted)'>Aucune PR ouverte sur les dépôts suivis.</span>"}
            </div>
          </div>
          ${renderRecentAgentReportsCard(snap, "btn-infra-open-agents", false)}
        </div>
      </div>
    </div>
  `;

  const btnAgentsInfra = document.getElementById("btn-infra-open-agents");
  if (btnAgentsInfra) {
    btnAgentsInfra.onclick = () => api("/api/screen/select", "POST", { target: "org-agents" });
  }
  document.getElementById("btn-refresh-infra").onclick = () => api("/api/org/refresh", "POST");
  const btnPveUpdate = document.getElementById("btn-proxmox-update");
  if (btnPveUpdate) {
    btnPveUpdate.onclick = async () => {
      btnPveUpdate.textContent = "⏳ Mise à jour Proxmox en cours…";
      const res = await api("/api/infra/proxmox/update", "POST");
      btnPveUpdate.textContent = res?.updatedTargets != null
        ? `✓ ${res.updatedTargets} nœuds/VMs lancés`
        : "✓ Lancé";
    };
  }

  document.getElementById("btn-ack-comms").onclick = () =>
    api("/api/comms/status", "POST", {
      missedCalls: 0,
      missedCallDetails: [],
      voicemails: 0,
      voicemailDetails: [],
      teamsUnread: 0,
      mailsUnread: 0,
      slackUnread: 0,
      slackMessages: [],
    });

  document.getElementById("btn-add-shop").onclick = () => {
    const input = document.getElementById("shop-new-name");
    const name = input.value.trim();
    if (!name) return;
    api("/api/home/shopping", "POST", { name, completed: false });
    input.value = "";
  };

  document.querySelectorAll(".shop-toggle").forEach((el) => {
    el.onchange = () => api(`/api/home/shopping/${encodeURIComponent(el.dataset.id)}/toggle`, "POST");
  });

  document.querySelectorAll(".shop-del").forEach((el) => {
    el.onclick = () => api(`/api/home/shopping/${encodeURIComponent(el.dataset.id)}`, "DELETE");
  });

  document.getElementById("btn-add-drive").onclick = () => {
    const store = document.getElementById("drive-new-store").value.trim() || "ClearDrive";
    const pickupSlot = document.getElementById("drive-new-slot").value.trim();
    api("/api/home/cleardrive", "POST", {
      store,
      status: "Prête au retrait",
      pickupSlot,
      readyForPickup: true,
    });
  };

  document.querySelectorAll(".drive-del").forEach((el) => {
    el.onclick = () => api(`/api/home/cleardrive/${encodeURIComponent(el.dataset.id)}`, "DELETE");
  });
}

function renderOrgAgenda(stage, snap) {
  const mails = snap.org?.mails || [];
  const weekDays = getWeekDays(7);
  const events = getRollingEvents(snap, weekDays);
  const icalUrl = snap.settings?.org?.icalUrl || "";
  const ed = snap.org?.ecoledirecte;
  const todayIso = toLocalIsoDate(new Date());

  const edBadge = ed
    ? (ed.connected
        ? `<span class="badge" style="background:rgba(168,85,247,0.2);color:#e9d5ff;">🎓 ÉcoleDirecte (${escapeHtml(ed.studentId)}) : ${ed.coursesCount} cours</span>`
        : `<span class="badge" style="background:rgba(245,158,11,0.18);color:#fde68a;" title="${escapeHtml(ed.lastError || "")}">🎓 ÉcoleDirecte : ${ed.configured ? escapeHtml(ed.lastError || "Erreur") : "Non configuré"}</span>`)
    : "";

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <div>
          <h2>Planning 7 jours glissants & ÉcoleDirecte</h2>
          <div class="row" style="margin-top:4px;">
            <span style="color:var(--muted);font-size:13px;">${icalUrl ? "Agenda iCal connecté • Synchro auto" : "Ajoutez un lien iCal ou configurez ÉcoleDirecte"}</span>
            ${edBadge}
          </div>
        </div>
        <div class="row">
          ${!icalUrl ? `<button class="btn" id="btn-agenda-open-settings">⚙️ Connecter iCal (.ics)</button>` : ""}
          <button class="btn primary" id="btn-org-refresh">↻ Actualiser Météo & iCal</button>
        </div>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <div class="row" style="justify-content:space-between;margin-bottom:12px;">
          <div>
            <h3>📅 7 prochains jours (glissant à partir d'aujourd'hui)</h3>
            <div class="row" style="gap:8px;margin-top:6px;font-size:11px;">
              <span class="badge" style="background:rgba(56,189,248,0.16);color:var(--primary);">Aujourd'hui</span>
              <span class="badge" style="background:rgba(139,92,246,0.18);color:#c4b5fd;">Week-end</span>
              <span class="badge" style="background:rgba(245,158,11,0.20);color:#fde68a;">🇫🇷 Jour férié</span>
              <span class="badge" style="background:rgba(34,197,94,0.16);color:#86efac;">Famille / Récurrent</span>
              <span class="badge" style="background:rgba(168,85,247,0.2);color:#e9d5ff;">🎓 Début → Fin ÉcoleDirecte</span>
            </div>
          </div>
          <div class="row">
            <input id="evt-title" placeholder="Nouvel événement…" style="width:200px;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:7px 10px;border-radius:8px;" />
            <input id="evt-date" type="date" value="${todayIso}" style="background:var(--bg);border:1px solid var(--border);color:var(--text);padding:7px 10px;border-radius:8px;" />
            <input id="evt-time" placeholder="09:00" style="width:74px;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:7px 10px;border-radius:8px;" />
            <button class="btn primary" id="btn-add-evt">+ Ajouter</button>
          </div>
        </div>
        <div class="week-grid">
          ${weekDays.map((d) => {
            const dayEvents = events.filter((e) => e.date === d.iso);
            const edBounds = getEdDayBounds(dayEvents);
            const dayClass = [
              "week-day",
              d.isToday ? "today" : "",
              d.isWeekend ? "weekend" : "",
              d.isHoliday ? "holiday" : ""
            ].filter(Boolean).join(" ");
            const titleColor = d.isToday
              ? "var(--primary)"
              : d.isHoliday
                ? "#fde68a"
                : d.isWeekend
                  ? "#c4b5fd"
                  : "var(--text)";
            const headBadge = d.isToday
              ? `<span style="font-size:10px;background:var(--primary);color:#041e2b;padding:1px 6px;border-radius:999px;">Auj.</span>`
              : d.isHoliday
                ? `<span style="font-size:10px;background:rgba(245,158,11,0.28);color:#fde68a;padding:1px 6px;border-radius:999px;">Férié</span>`
                : d.isWeekend
                  ? `<span style="font-size:10px;background:rgba(139,92,246,0.28);color:#ddd6fe;padding:1px 6px;border-radius:999px;">WE</span>`
                  : `<span style="color:var(--muted);font-weight:400;">${dayEvents.length || ""}</span>`;
            return `
              <div class="${dayClass}" data-date="${d.iso}">
                <div class="week-day-head">
                  <span style="color:${titleColor}">${d.weekday} ${d.dayNum} ${escapeHtml(d.monthShort)}</span>
                  ${headBadge}
                </div>
                ${d.holidayName ? `<div style="font-size:11px;color:#fde68a;font-weight:700;">🇫🇷 ${escapeHtml(d.holidayName)}</div>` : ""}
                ${edBounds.length ? `
                  <div class="ed-bounds-box">
                    <div style="color:#e9d5ff;font-weight:700;margin-bottom:2px;">🏫 ÉcoleDirecte</div>
                    ${edBounds.map((b) => `
                      <div style="display:flex;justify-content:space-between;gap:4px;">
                        <span>${escapeHtml(b.child)}</span>
                        <strong>${escapeHtml(b.startTime)} → ${escapeHtml(b.endTime)}</strong>
                      </div>
                    `).join("")}
                  </div>
                ` : ""}
                ${dayEvents.length ? dayEvents.map((e) => {
                  const isEd = e.source === "ecoledirecte";
                  const isRecurring = e.source === "recurring";
                  const isCancelled = e.title.startsWith("[ANNULÉ]");
                  const timeColor = isEd ? "#c084fc" : isRecurring ? "#86efac" : "var(--primary)";
                  return `
                  <div class="evt-chip ${isEd ? "ed" : ""} ${isRecurring ? "recurring" : ""} ${isCancelled ? "cancelled" : ""}">
                    <div style="display:flex;justify-content:space-between;gap:4px;align-items:center;">
                      <strong style="color:${timeColor};">${escapeHtml(e.startTime || "Journée")}${e.endTime ? "-" + escapeHtml(e.endTime) : ""}</strong>
                      ${e.source !== "ical" && !isEd && !isRecurring ? `<button class="btn danger evt-del-btn" data-id="${escapeHtml(e.id)}" style="padding:1px 5px;font-size:10px;">✕</button>` : ""}
                    </div>
                    <div style="margin-top:2px;">${isEd ? "🎓 " : ""}${escapeHtml(e.title)}</div>
                    ${e.location ? `<div style="color:var(--muted);font-size:11px;">📍 ${escapeHtml(e.location)}</div>` : ""}
                  </div>
                `;
                }).join("") : `<div style="color:var(--muted);font-size:12px;margin:auto 0;text-align:center;opacity:0.7;">Libre</div>`}
              </div>
            `;
          }).join("")}
        </div>
      </div>

      <div class="grid-2" style="height:auto;">
        <div class="col">
          ${renderWeatherCard(snap.org?.weather)}
        </div>
        <div class="col">
          <div class="card">
            <h3 style="margin-bottom:10px;">📧 Mails du jour</h3>
            <div class="row" style="margin-bottom:12px;">
              <input id="mail-sender" placeholder="Expéditeur" style="flex:1;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
              <input id="mail-subject" placeholder="Objet du mail…" style="flex:2;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
              <button class="btn primary" id="btn-add-mail">+</button>
            </div>
            ${mails.length ? mails.map((m) => `
              <div class="note-row">
                <div>
                  <div><strong>${escapeHtml(m.sender)}</strong> <span style="color:var(--muted);font-size:12px;">• ${escapeHtml(m.receivedAt)}</span></div>
                  <div>${m.important ? "⭐ " : ""}${escapeHtml(m.subject)}</div>
                  ${m.snippet ? `<div style="color:var(--muted);font-size:12px;">${escapeHtml(m.snippet)}</div>` : ""}
                </div>
                <button class="btn danger" onclick="api('/api/org/mails/${encodeURIComponent(m.id)}','DELETE')">Suppr.</button>
              </div>
            `).join("") : "<p style='color:var(--muted)'>Aucun mail du jour.</p>"}
          </div>
        </div>
      </div>
    </div>
  `;

  document.getElementById("btn-org-refresh").onclick = () => api("/api/org/refresh", "POST");
  const openCfgBtn = document.getElementById("btn-agenda-open-settings");
  if (openCfgBtn) {
    openCfgBtn.onclick = () => document.getElementById("btn-settings").click();
  }
  document.querySelectorAll(".week-day").forEach((col) => {
    col.onclick = (ev) => {
      if (ev.target.closest(".evt-del-btn")) return;
      document.getElementById("evt-date").value = col.dataset.date;
      document.getElementById("evt-title").focus();
    };
  });
  document.querySelectorAll(".evt-del-btn").forEach((btn) => {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      api(`/api/org/events/${encodeURIComponent(btn.dataset.id)}`, "DELETE");
    };
  });
  document.getElementById("btn-add-evt").onclick = () => {
    const title = document.getElementById("evt-title").value.trim();
    const date = document.getElementById("evt-date").value;
    const startTime = document.getElementById("evt-time").value.trim();
    if (title && date) api("/api/org/events", "POST", { title, date, startTime });
  };
  document.getElementById("btn-add-mail").onclick = () => {
    const sender = document.getElementById("mail-sender").value.trim() || "Expéditeur";
    const subject = document.getElementById("mail-subject").value.trim();
    if (subject) api("/api/org/mails", "POST", { sender, subject });
  };
}

function renderOrgNotes(stage, snap) {
  stage.innerHTML = `
    <div class="org-view">
      <h2 style="margin-bottom:16px;">Organisation — Notes & Checklist</h2>
      <div class="card" style="margin-bottom:16px;">
        <div class="row">
          <input id="note-input" placeholder="Nouvelle note ou tâche…" style="flex:1;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:10px;border-radius:8px;" />
          <select id="note-cat" style="background:var(--bg);border:1px solid var(--border);color:var(--text);padding:10px;border-radius:8px;">
            <option>Organisation</option><option>Observabilité</option><option>Incident</option><option>Perso</option>
          </select>
          <button class="btn primary" id="btn-add-note">Ajouter</button>
        </div>
      </div>
      <div id="notes-list"></div>
    </div>
  `;
  document.getElementById("btn-add-note").onclick = () => {
    const text = document.getElementById("note-input").value.trim();
    const category = document.getElementById("note-cat").value;
    if (text) api("/api/notes", "POST", { text, category });
  };
  const list = document.getElementById("notes-list");
  snap.notes.forEach((n) => {
    const row = document.createElement("div");
    row.className = "note-row" + (n.done ? " done" : "");
    row.innerHTML = `
      <div class="row" style="flex:1">
        <input type="checkbox" ${n.done ? "checked" : ""} />
        <span class="txt">${escapeHtml(n.text)}</span>
        <span style="color:var(--primary);font-size:12px;">${escapeHtml(n.category)}</span>
      </div>
      <button class="btn danger">Suppr.</button>
    `;
    row.querySelector("input").onchange = () => api(`/api/notes/${encodeURIComponent(n.id)}/toggle`, "POST");
    row.querySelector("button").onclick = () => api(`/api/notes/${encodeURIComponent(n.id)}`, "DELETE");
    list.appendChild(row);
  });
}

function renderOrgClock(stage) {
  const now = new Date();
  stage.innerHTML = `
    <div class="org-view" style="display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;">
      <div style="font-size:110px;font-weight:800;" id="focus-clock">${now.toLocaleTimeString("fr-FR")}</div>
      <div class="date-sub">${now.toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</div>
    </div>
  `;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function closeModal(id) { document.getElementById(id).classList.remove("open"); }

document.getElementById("btn-rotation").onclick = () => {
  if (!currentSnapshot) return;
  const s = { ...currentSnapshot.settings, rotationEnabled: !currentSnapshot.settings.rotationEnabled };
  api("/api/settings", "POST", s);
};
document.getElementById("btn-reload").onclick = () => api("/api/reload", "POST");
document.getElementById("btn-alert-ack").onclick = () => api("/api/alert/ack", "POST");
document.getElementById("btn-alert-clear").onclick = () => api("/api/alert/clear", "POST");
document.getElementById("btn-hide-hud").onclick = () => {
  document.getElementById("hud").classList.add("hidden");
  document.getElementById("btn-show-hud").classList.add("visible");
};
document.getElementById("btn-show-hud").onclick = () => {
  document.getElementById("hud").classList.remove("hidden");
  document.getElementById("btn-show-hud").classList.remove("visible");
};
const collapseBtn = document.getElementById("btn-collapse-sidebar");
if (collapseBtn) {
  if (localStorage.getItem("iris_sidebar_collapsed") === "1") {
    document.getElementById("hud").classList.add("collapsed");
    collapseBtn.textContent = "▶";
  }
  collapseBtn.onclick = () => {
    const hud = document.getElementById("hud");
    const isCollapsed = hud.classList.toggle("collapsed");
    collapseBtn.textContent = isCollapsed ? "▶" : "◀";
    localStorage.setItem("iris_sidebar_collapsed", isCollapsed ? "1" : "0");
  };
}

document.getElementById("btn-screens").onclick = () => {
  if (!currentSnapshot) return;
  const list = document.getElementById("screens-list");
  list.innerHTML = currentSnapshot.screens.map((s) => `
    <div class="note-row">
      <div><strong>${escapeHtml(s.name)}</strong> <span style="color:var(--primary);font-size:12px;">(${s.type})</span> ${s.isAlertTarget ? "⚠️" : ""}<div style="color:var(--muted);font-size:12px;">${escapeHtml(s.url || "")}</div></div>
      <button class="btn danger" onclick="api('/api/screens/${encodeURIComponent(s.id)}','DELETE')">Suppr.</button>
    </div>
  `).join("");
  document.getElementById("modal-screens").classList.add("open");
};

document.getElementById("btn-save-screen").onclick = () => {
  const id = document.getElementById("scr-id").value.trim() || `scr-${Date.now()}`;
  const name = document.getElementById("scr-name").value.trim() || "Écran";
  const type = document.getElementById("scr-type").value;
  const url = document.getElementById("scr-url").value.trim();
  const isAlertTarget = document.getElementById("scr-alert").checked;
  const enabled = document.getElementById("scr-enabled").checked;
  api("/api/screens", "POST", { id, name, type, url, kioskMode: true, autoRefreshSeconds: 30, enabled, isAlertTarget });
  closeModal("modal-screens");
};

document.getElementById("btn-settings").onclick = () => {
  if (!currentSnapshot) return;
  const s = currentSnapshot.settings;
  document.getElementById("cfg-host").value = s.mqtt.brokerHost;
  document.getElementById("cfg-port").value = s.mqtt.brokerPort;
  document.getElementById("cfg-device").value = s.mqtt.deviceId;
  document.getElementById("cfg-topic").value = s.mqtt.baseTopic;
  document.getElementById("cfg-rot-sec").value = s.rotationIntervalSeconds;
  document.getElementById("cfg-weather-city").value = s.org?.weatherCity || "Lyon";
  document.getElementById("cfg-ical-url").value = s.org?.icalUrl || "";
  document.getElementById("cfg-github-repos").value = (s.org?.githubRepos || []).join(", ");
  document.getElementById("cfg-mqtt-enabled").checked = s.mqtt.enabled;
  document.getElementById("cfg-rot-enabled").checked = s.rotationEnabled;
  document.getElementById("modal-settings").classList.add("open");
};

document.getElementById("btn-save-settings").onclick = () => {
  if (!currentSnapshot) return;
  const s = structuredClone(currentSnapshot.settings);
  s.mqtt.brokerHost = document.getElementById("cfg-host").value.trim();
  s.mqtt.brokerPort = parseInt(document.getElementById("cfg-port").value, 10) || 1883;
  s.mqtt.deviceId = document.getElementById("cfg-device").value.trim() || "tablet-1";
  s.mqtt.baseTopic = document.getElementById("cfg-topic").value.trim() || "iris";
  s.mqtt.enabled = document.getElementById("cfg-mqtt-enabled").checked;
  s.rotationEnabled = document.getElementById("cfg-rot-enabled").checked;
  s.rotationIntervalSeconds = parseInt(document.getElementById("cfg-rot-sec").value, 10) || 45;
  s.org = s.org || { weatherCity: "Lyon", weatherLat: "45.7640", weatherLon: "4.8357", icalUrl: "", githubRepos: [] };
  s.org.weatherCity = document.getElementById("cfg-weather-city").value.trim() || "Lyon";
  s.org.icalUrl = document.getElementById("cfg-ical-url").value.trim();
  s.org.githubRepos = document
    .getElementById("cfg-github-repos")
    .value.split(",")
    .map((r) => r.trim())
    .filter(Boolean);
  api("/api/settings", "POST", s);
  closeModal("modal-settings");
};

document.getElementById("btn-sim-alert").onclick = () => {
  api("/api/alert/trigger", "POST", {
    title: "Test Incident — OpenObserve / jo3",
    message: "Bascule automatique vers le dashboard d'alerte.",
    severity: "CRITICAL",
    source: "simulation"
  });
  closeModal("modal-settings");
};

if (!clockTimer) {
  clockTimer = setInterval(() => {
    const hub = document.getElementById("hub-clock");
    if (hub) hub.textContent = new Date().toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
    const foc = document.getElementById("focus-clock");
    if (foc) foc.textContent = new Date().toLocaleTimeString("fr-FR");
  }, 1000);
}

connectWs();
