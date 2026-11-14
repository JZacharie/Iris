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
  else if (screen.type === "ORG_AGENDA") renderOrgAgenda(stage, snap);
  else if (screen.type === "ORG_NOTES") renderOrgNotes(stage, snap);
  else if (screen.type === "ORG_CLOCK") renderOrgClock(stage);
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

function renderOrgOverview(stage, snap) {
  const now = new Date();
  const timeStr = now.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  const dateStr = now.toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  const notes = snap.notes.filter((n) => !n.done).slice(0, 4);
  const mails = (snap.org?.mails || []).slice(0, 4);
  const events = (snap.org?.events || []).slice(0, 4);

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
            <h3>📅 Planning de la semaine (${events.length})</h3>
            <div style="margin-top:8px;">
              ${events.length ? events.map((e) => `<div style="margin-bottom:6px;"><span style="color:var(--primary);font-weight:600;">${escapeHtml(formatShortDate(e.date))} ${escapeHtml(e.startTime)}</span> — ${escapeHtml(e.title)}</div>`).join("") : "<span style='color:var(--muted)'>Aucun événement configuré (ajoutez une URL .ics dans Réglages).</span>"}
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
  const launchers = document.getElementById("hub-launchers");
  snap.screens.filter((s) => s.enabled).forEach((s) => {
    const btn = document.createElement("button");
    btn.className = "launcher";
    btn.innerHTML = `<strong>${escapeHtml(s.name)}</strong><div style="color:var(--muted);font-size:12px;margin-top:4px;">${s.type}</div>`;
    btn.onclick = () => api("/api/screen/select", "POST", { target: s.id });
    launchers.appendChild(btn);
  });
}

function renderOrgAgenda(stage, snap) {
  const mails = snap.org?.mails || [];
  const events = snap.org?.events || [];

  stage.innerHTML = `
    <div class="org-view">
      <div class="row" style="justify-content:space-between;margin-bottom:16px;">
        <h2>Planning de la semaine & Mails du jour</h2>
        <button class="btn primary" id="btn-org-refresh">↻ Actualiser Météo & iCal</button>
      </div>
      ${renderWeatherCard(snap.org?.weather)}
      <div class="grid-2" style="margin-top:16px;height:auto;">
        <div class="col">
          <div class="card">
            <h3 style="margin-bottom:10px;">📅 Planning de la semaine</h3>
            <div class="row" style="margin-bottom:12px;">
              <input id="evt-title" placeholder="Événement…" style="flex:2;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
              <input id="evt-date" type="date" value="${new Date().toISOString().slice(0, 10)}" style="background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
              <input id="evt-time" placeholder="09:00" style="width:72px;background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px;border-radius:8px;" />
              <button class="btn primary" id="btn-add-evt">+</button>
            </div>
            ${events.length ? events.map((e) => `
              <div class="note-row">
                <div>
                  <strong style="color:var(--primary);">${escapeHtml(formatShortDate(e.date))} ${escapeHtml(e.startTime)}${e.endTime ? "-" + escapeHtml(e.endTime) : ""}</strong>
                  <div>${escapeHtml(e.title)} ${e.location ? `<span style="color:var(--muted);font-size:12px;">(${escapeHtml(e.location)})</span>` : ""}</div>
                </div>
                ${e.source !== "ical" ? `<button class="btn danger" onclick="api('/api/org/events/${encodeURIComponent(e.id)}','DELETE')">Suppr.</button>` : `<span style="font-size:11px;color:var(--muted);">iCal</span>`}
              </div>
            `).join("") : "<p style='color:var(--muted)'>Aucun événement cette semaine.</p>"}
          </div>
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
  s.org = s.org || { weatherCity: "Lyon", weatherLat: "45.7640", weatherLon: "4.8357", icalUrl: "" };
  s.org.weatherCity = document.getElementById("cfg-weather-city").value.trim() || "Lyon";
  s.org.icalUrl = document.getElementById("cfg-ical-url").value.trim();
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
