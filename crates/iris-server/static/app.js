let currentSnapshot = null;
let lastRenderedKey = null;
let clockTimer = null;

async function api(path, method = "GET", body = null) {
  const opts = { method, headers: { "Content-Type": "application/json" } };
  if (body !== null) opts.body = JSON.stringify(body);
  const res = await fetch(path, opts);
  return res.json();
}

function connectWs() {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${proto}//${location.host}/ws`);
  ws.onmessage = (ev) => {
    try {
      currentSnapshot = JSON.parse(ev.data);
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

function renderHud(snap) {
  const pills = document.getElementById("screen-pills");
  pills.innerHTML = `<span class="brand">IRIS</span>`;
  const displayedId = snap.displayedScreen?.id;
  snap.screens.filter((s) => s.enabled).forEach((s) => {
    const b = document.createElement("button");
    b.className = "pill" + (s.id === displayedId && !snap.transientScreen ? " active" : "");
    b.textContent = s.name;
    b.onclick = () => api("/api/screen/select", "POST", { target: s.id });
    pills.appendChild(b);
  });
  if (snap.transientScreen) {
    const t = document.createElement("button");
    t.className = "pill active";
    t.textContent = snap.transientScreen.name;
    pills.appendChild(t);
  }

  const dot = document.getElementById("mqtt-dot");
  dot.className = "dot " + snap.mqttState;
  document.getElementById("mqtt-label").textContent = `MQTT: ${snap.mqttState}`;
  document.getElementById("btn-rotation").textContent = snap.settings.rotationEnabled ? "Pause" : "Lecture";

  const barthes = snap.barthesState || "DEGRE_ZERO";
  const barthesMeta = {
    DEGRE_ZERO: { label: "Degré zéro", desc: "Équilibre silencieux — flux nominal" },
    STUDIUM: { label: "Studium", desc: "Attention calme — veille organisationnelle" },
    CONNOTATION: { label: "Connotation", desc: "Signal faible — tension ressources ou quota" },
    PRESENCE: { label: "Présence", desc: "En communication — Teams / Zoom actif" },
    PUNCTUM: { label: "Punctum", desc: "Rupture critique — incident actif" }
  }[barthes] || { label: "Degré zéro", desc: "" };
  const pill = document.getElementById("barthes-pill");
  if (pill) {
    pill.className = `barthes-pill barthes-${barthes}`;
    pill.textContent = barthesMeta.label;
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

function renderStage(snap) {
  const screen = snap.displayedScreen;
  const stage = document.getElementById("stage");
  if (!screen) {
    stage.innerHTML = `<div class="org-view"><h2>Aucun écran configuré</h2></div>`;
    return;
  }

  const key = `${screen.id}|${snap.formattedUrl}|${snap.reloadCounter}|${screen.type}`;
  if (screen.type === "GRAFANA" || screen.type === "OPENOBSERVE" || screen.type === "WEB") {
    if (lastRenderedKey === key) return;
    lastRenderedKey = key;
    if (!snap.formattedUrl) {
      stage.innerHTML = `<div class="org-view"><h2>Aucune URL configurée pour ${screen.name}</h2></div>`;
      return;
    }
    stage.innerHTML = `<iframe class="dashboard-frame" src="${snap.formattedUrl}" allow="fullscreen"></iframe>`;
    return;
  }

  lastRenderedKey = null;
  if (screen.type === "ORG_OVERVIEW") renderOrgOverview(stage, snap);
  else if (screen.type === "ORG_LIFE_INFRA") renderLifeInfra(stage, snap);
  else if (screen.type === "ORG_STREAM_DECK") renderStreamDeck(stage, snap);
  else if (screen.type === "ORG_AGENDA") renderOrgAgenda(stage, snap);
  else if (screen.type === "ORG_NOTES") renderOrgNotes(stage, snap);
  else if (screen.type === "ORG_CLOCK") renderOrgClock(stage);
}

function renderStreamDeck(stage, snap) {
  const pc = snap.pcTelemetry;
  const buttons = snap.deckButtons || [];
  const cpu = pc ? pc.cpuPercent : 0;
  const ramPct = pc ? pc.ramPercent : 0;
  const ramUsedGb = pc ? (pc.ramUsedMb / 1024).toFixed(1) : "0.0";
  const ramTotalGb = pc ? (pc.ramTotalMb / 1024).toFixed(1) : "0.0";
  const limit = pc?.antigravityTokensLimit || 2000000;
  const used = pc?.antigravityTokensUsed || 0;
  const remaining = Math.max(0, limit - used);
  const quotaPct = Math.min(100, Math.round((remaining * 100) / limit));

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <div>
          <h2>Stream Deck & Télémétrie PC</h2>
          <div style="color:var(--muted);font-size:13px;">${pc ? `Poste : ${escapeHtml(pc.hostname)} • Uptime ${Math.floor(pc.uptimeSecs / 3600)}h` : "En attente de iris-pc-agent sur votre PC…"}</div>
        </div>
        <button class="btn" id="btn-toggle-deck-add">+ Ajouter une touche</button>
      </div>

      <div class="deck-telemetry">
        <div class="metric-card">
          <div style="color:var(--muted);font-size:12px;">CPU</div>
          <div style="font-size:28px;font-weight:800;margin-top:4px;">${pc ? cpu + "%" : "—"}</div>
          <div class="bar-track"><div class="bar-fill" style="width:${cpu}%;background:${cpu >= 85 ? "var(--crit)" : "var(--primary)"};"></div></div>
        </div>
        <div class="metric-card">
          <div style="color:var(--muted);font-size:12px;">Mémoire RAM</div>
          <div style="font-size:28px;font-weight:800;margin-top:4px;">${pc ? ramPct + "%" : "—"}</div>
          <div style="font-size:12px;color:var(--muted);">${pc ? `${ramUsedGb} / ${ramTotalGb} Go` : "Hors ligne"}</div>
          <div class="bar-track"><div class="bar-fill" style="width:${ramPct}%;background:${ramPct >= 85 ? "var(--crit)" : "var(--ok)"};"></div></div>
        </div>
        <div class="metric-card">
          <div style="color:var(--muted);font-size:12px;">Quota Antigravity restant</div>
          <div style="font-size:24px;font-weight:800;margin-top:4px;">${(remaining / 1000).toFixed(0)}k <span style="font-size:13px;color:var(--muted);">/ ${(limit / 1000).toFixed(0)}k</span></div>
          <div style="font-size:12px;color:var(--primary);">${escapeHtml(pc?.antigravityModel || "Gemini 4 Hydrogen")}</div>
          <div class="bar-track"><div class="bar-fill" style="width:${quotaPct}%;background:var(--primary);"></div></div>
        </div>
        <div class="metric-card">
          <div style="color:var(--muted);font-size:12px;">Communication</div>
          <div style="font-size:22px;font-weight:800;margin-top:6px;">${pc?.activeMeeting ? "🟢 " + escapeHtml(pc.activeMeeting) : "⚪ Veille"}</div>
          <div style="font-size:12px;color:var(--muted);margin-top:6px;">${snap.lastDeckAction ? "Dernière action : " + escapeHtml(snap.lastDeckAction.label) : "Prêt à exécuter"}</div>
        </div>
      </div>

      <div id="deck-add-form" class="card" style="display:none;margin-bottom:16px;">
        <div class="row">
          <input id="dk-label" placeholder="Nom (ex: Daily Teams)" style="flex:1;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
          <input id="dk-icon" placeholder="Icône (📞)" style="width:90px;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
          <select id="dk-kind" style="background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;">
            <option value="JOIN_TEAMS">Rejoindre Teams</option>
            <option value="JOIN_ZOOM">Rejoindre Zoom</option>
            <option value="OPEN_URL">Ouvrir URL</option>
            <option value="LAUNCH_APP">Lancer App</option>
            <option value="MEDIA_TOGGLE">Média Play/Pause</option>
            <option value="LOCK_SCREEN">Verrouiller PC</option>
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
        <div style="font-size:12px;color:var(--muted);margin-top:2px;">${escapeHtml(btn.subtitle || "")}</div>
      </div>
    `;
    el.onclick = () => api("/api/deck/trigger", "POST", { buttonId: btn.id });
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

function getWeekDays() {
  const now = new Date();
  const todayIso = toLocalIsoDate(now);
  const day = now.getDay(); // 0=Sun..6=Sat
  const diffToMon = (day === 0 ? -6 : 1) - day;
  const monday = new Date(now.getFullYear(), now.getMonth(), now.getDate() + diffToMon);
  const days = [];
  for (let i = 0; i < 7; i++) {
    const d = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + i);
    const iso = toLocalIsoDate(d);
    const weekday = d.toLocaleDateString("fr-FR", { weekday: "short" }).replace(".", "");
    days.push({
      iso,
      dayNum: d.getDate(),
      weekday: weekday.charAt(0).toUpperCase() + weekday.slice(1),
      isToday: iso === todayIso,
    });
  }
  return days;
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
  const events = (snap.org?.events || []).slice(0, 4);
  const comms = snap.comms || {};
  const shopPending = (snap.shoppingList || []).filter((i) => !i.completed).length;
  const driveReady = (snap.driveOrders || []).filter((o) => o.readyForPickup).length;
  const cluster = snap.infra?.cluster;
  const offlineCount = cluster?.offlineNodes?.length || 0;
  const openPrs = snap.infra?.openPrs || [];

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
              <div>✉️ Mails non lus : <strong>${comms.mailsUnread || 0}</strong></div>
              <div>🛒 Courses Alexa : <strong>${shopPending}</strong></div>
              <div>🚗 ClearDrive prêt : <strong style="color:${driveReady > 0 ? "var(--ok)" : "inherit"}">${driveReady}</strong></div>
              <div>☸️ Cluster jo3 : <strong style="color:${offlineCount > 0 ? "var(--crit)" : "var(--ok)"}">${cluster ? `${cluster.nodesReady}/${cluster.nodesTotal} Ready` : "—"}</strong></div>
              <div>🔀 PRs GitHub : <strong>${openPrs.length}</strong></div>
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
              <h3>📅 Planning de la semaine (${events.length})</h3>
              <button class="btn" id="btn-hub-open-agenda" style="padding:4px 10px;font-size:12px;">Voir / Ajouter →</button>
            </div>
            <div class="week-strip">
              ${getWeekDays().map((d) => {
                const count = (snap.org?.events || []).filter((e) => e.date === d.iso).length;
                return `
                  <div class="week-pill ${d.isToday ? "today" : ""}">
                    <div style="color:${d.isToday ? "var(--primary)" : "var(--muted)"};font-weight:700;">${d.weekday}</div>
                    <div style="font-size:14px;font-weight:800;margin:2px 0;">${d.dayNum}</div>
                    <div style="color:${count > 0 ? "var(--ok)" : "var(--muted)"};font-weight:600;">${count > 0 ? `${count} evt` : "Libre"}</div>
                  </div>
                `;
              }).join("")}
            </div>
            <div style="margin-top:10px;">
              ${events.length ? events.map((e) => `<div style="margin-bottom:6px;"><span style="color:var(--primary);font-weight:600;">${escapeHtml(formatShortDate(e.date))} ${escapeHtml(e.startTime)}</span> — ${escapeHtml(e.title)}</div>`).join("") : "<span style='color:var(--muted);font-size:13px;'>Aucun événement saisi cette semaine. Cliquez sur « Voir / Ajouter » ou connectez votre URL .ics.</span>"}
            </div>
          </div>
          <div class="card">
            <h3>📌 Notes prioritaires</h3>
            <div style="margin-top:8px;">
              ${notes.length ? notes.map((n) => `<div style="margin-bottom:4px;">${n.pinned ? "📌 " : "• "}${escapeHtml(n.text)}</div>`).join("") : "<span style='color:var(--muted)'>Aucune tâche en attente.</span>"}
            </div>
          </div>
        </div>
      </div>
    </div>
  `;
  const btnAgenda = document.getElementById("btn-hub-open-agenda");
  if (btnAgenda) {
    btnAgenda.onclick = () => api("/api/screen/select", "POST", { target: "org-agenda" });
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

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <div>
          <h2>Vie Quotidienne, Messagerie & Infra jo3</h2>
          <div style="color:var(--muted);font-size:13px;">Téléphone, Teams, Mails, Alexa, ClearDrive, nœuds Kubernetes jo3 & PRs GitHub</div>
        </div>
        <button class="btn primary" id="btn-refresh-infra">↻ Actualiser Infra & PRs</button>
      </div>

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
                <div style="color:var(--muted);font-size:12px;">Teams non lus</div>
                <div style="font-size:26px;font-weight:800;">${comms.teamsUnread || 0}</div>
              </div>
              <div class="metric-card">
                <div style="color:var(--muted);font-size:12px;">Mails non lus</div>
                <div style="font-size:26px;font-weight:800;">${comms.mailsUnread || 0}</div>
              </div>
            </div>
            ${missedDetails.length ? `<div style="margin-top:10px;font-size:13px;"><strong>Appels manqués :</strong> ${missedDetails.map((d) => escapeHtml(d)).join(" • ")}</div>` : ""}
            ${voicemailDetails.length ? `<div style="margin-top:6px;font-size:13px;"><strong>Répondeur :</strong> ${voicemailDetails.map((d) => escapeHtml(d)).join(" • ")}</div>` : ""}
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
        </div>
      </div>
    </div>
  `;

  document.getElementById("btn-refresh-infra").onclick = () => api("/api/org/refresh", "POST");

  document.getElementById("btn-ack-comms").onclick = () =>
    api("/api/comms/status", "POST", {
      missedCalls: 0,
      missedCallDetails: [],
      voicemails: 0,
      voicemailDetails: [],
      teamsUnread: 0,
      mailsUnread: 0,
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
  const events = snap.org?.events || [];
  const weekDays = getWeekDays();
  const icalUrl = snap.settings?.org?.icalUrl || "";
  const todayIso = toLocalIsoDate(new Date());

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <div>
          <h2>Planning de la semaine & Mails du jour</h2>
          <div style="color:var(--muted);font-size:13px;">${icalUrl ? "Agenda iCal connecté • Synchro auto toutes les 10 min" : "Aucun agenda .ics configuré — ajoutez votre lien iCal ou créez des événements ci-dessous"}</div>
        </div>
        <div class="row">
          ${!icalUrl ? `<button class="btn" id="btn-agenda-open-settings">⚙️ Connecter iCal (.ics)</button>` : ""}
          <button class="btn primary" id="btn-org-refresh">↻ Actualiser Météo & iCal</button>
        </div>
      </div>

      <div class="card" style="margin-bottom:16px;">
        <div class="row" style="justify-content:space-between;margin-bottom:12px;">
          <h3>📅 Semaine en cours (Lun → Dim)</h3>
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
            return `
              <div class="week-day ${d.isToday ? "today" : ""}" data-date="${d.iso}">
                <div class="week-day-head">
                  <span style="color:${d.isToday ? "var(--primary)" : "var(--text)"}">${d.weekday} ${d.dayNum}</span>
                  ${d.isToday ? `<span style="font-size:10px;background:var(--primary);color:#041e2b;padding:1px 6px;border-radius:999px;">Auj.</span>` : `<span style="color:var(--muted);font-weight:400;">${dayEvents.length || ""}</span>`}
                </div>
                ${dayEvents.length ? dayEvents.map((e) => `
                  <div class="evt-chip">
                    <div style="display:flex;justify-content:space-between;gap:4px;align-items:center;">
                      <strong style="color:var(--primary);">${escapeHtml(e.startTime || "Journée")}${e.endTime ? "-" + escapeHtml(e.endTime) : ""}</strong>
                      ${e.source !== "ical" ? `<button class="btn danger evt-del-btn" data-id="${escapeHtml(e.id)}" style="padding:1px 5px;font-size:10px;">✕</button>` : ""}
                    </div>
                    <div style="margin-top:2px;">${escapeHtml(e.title)}</div>
                    ${e.location ? `<div style="color:var(--muted);font-size:11px;">${escapeHtml(e.location)}</div>` : ""}
                  </div>
                `).join("") : `<div style="color:var(--muted);font-size:12px;margin:auto 0;text-align:center;opacity:0.7;">Libre</div>`}
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
