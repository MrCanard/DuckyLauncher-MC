const { invoke } = window.__TAURI__.core;

let currentAccount = null;
let currentSearchType = "mod";

// ---------- Navigation ----------
document.querySelectorAll(".nav-btn").forEach((btn) => {
  btn.addEventListener("click", () => showView(btn.dataset.view));
});

function showView(name) {
  document.querySelectorAll(".view").forEach((v) => v.classList.remove("active"));
  document.querySelectorAll(".nav-btn").forEach((b) => b.classList.remove("active"));
  document.getElementById(`view-${name}`).classList.add("active");
  const navBtn = document.querySelector(`.nav-btn[data-view="${name}"]`);
  if (navBtn) navBtn.classList.add("active");

  if (name === "home") refreshHome();
  if (name === "library") refreshLibrary();
  if (name === "servers") refreshServers();
}

// ---------- Auth (device code flow Microsoft) ----------
document.getElementById("login-btn").addEventListener("click", async () => {
  const box = document.getElementById("account-box");
  try {
    const resp = await invoke("start_login");
    box.innerHTML = `
      <p>Va sur <b>${resp.verification_uri}</b><br/>
      et entre le code : <b>${resp.user_code}</b></p>
    `;
    pollLogin(resp.interval * 1000);
  } catch (e) {
    box.innerHTML = `<p style="color:#f66">Erreur: ${e}</p>`;
  }
});

async function pollLogin(intervalMs) {
  const box = document.getElementById("account-box");
  const timer = setInterval(async () => {
    try {
      const account = await invoke("poll_login");
      if (account) {
        clearInterval(timer);
        currentAccount = account;
        box.innerHTML = `
          <div class="account-chip">
            <div class="avatar"></div>
            <div><b>${account.mc_username}</b><br/><span style="color:var(--muted);font-size:11px">Compte Microsoft</span></div>
          </div>`;
      }
    } catch (e) {
      clearInterval(timer);
      box.innerHTML = `<p style="color:#f66">Erreur: ${e}</p>`;
    }
  }, intervalMs);
}

async function restoreAccount() {
  const accounts = await invoke("list_accounts");
  if (accounts.length > 0) {
    currentAccount = accounts[0];
    document.getElementById("account-box").innerHTML = `
      <div class="account-chip">
        <div class="avatar"></div>
        <div><b>${currentAccount.mc_username}</b><br/><span style="color:var(--muted);font-size:11px">Compte Microsoft</span></div>
      </div>`;
  }
}

// ---------- Accueil ----------
async function refreshHome() {
  const instances = await invoke("list_instances");
  const grid = document.getElementById("home-grid");
  grid.innerHTML = instances.length
    ? ""
    : `<p class="hint">Aucune instance pour l'instant — clique sur "+" pour en créer une.</p>`;
  for (const inst of instances.slice(0, 8)) {
    grid.appendChild(instanceCard(inst));
  }
  renderPinned(instances.slice(0, 6));
}

function renderPinned(instances) {
  const pinned = document.getElementById("pinned-instances");
  pinned.innerHTML = "";
  for (const inst of instances) {
    const el = document.createElement("div");
    el.className = "pinned-item";
    el.innerHTML = `<span class="dot"></span>${inst.name}`;
    el.addEventListener("click", () => launchInstance(inst));
    pinned.appendChild(el);
  }
}

// ---------- Bibliothèque ----------
async function refreshLibrary() {
  const instances = await invoke("list_instances");
  const grid = document.getElementById("library-grid");
  grid.innerHTML = instances.length
    ? ""
    : `<p class="hint">Aucune instance. Crée-en une pour commencer à jouer.</p>`;
  for (const inst of instances) {
    grid.appendChild(instanceCard(inst));
  }
}

function instanceCard(inst) {
  const card = document.createElement("div");
  card.className = "card";
  card.innerHTML = `
    <div class="icon-fallback">🧊</div>
    <h3>${inst.name}</h3>
    <p>${inst.mc_version} · ${loaderLabel(inst.loader)}</p>
    <p>${inst.mods.length} mod(s)</p>
    <button class="play-btn">▶ Jouer</button>
  `;
  card.querySelector(".play-btn").addEventListener("click", (e) => {
    e.stopPropagation();
    launchInstance(inst);
  });
  return card;
}

function loaderLabel(loader) {
  if (typeof loader === "string") return loader;
  return "Vanilla";
}

async function launchInstance(inst) {
  if (!currentAccount) {
    alert("Connecte-toi d'abord avec un compte Microsoft.");
    return;
  }
  try {
    await invoke("launch_instance", { instanceId: inst.id, accountUuid: currentAccount.mc_uuid });
  } catch (e) {
    alert("Erreur au lancement : " + e);
  }
}

// ---------- Nouvelle instance ----------
async function openNewInstanceModal() {
  showView("new-instance");
  const manifest = await invoke("list_mc_versions");
  const select = document.getElementById("ni-version");
  select.innerHTML = manifest.versions
    .filter((v) => v.version_type === "release")
    .map((v) => `<option value="${v.id}">${v.id}</option>`)
    .join("");
}
document.getElementById("new-instance-btn").addEventListener("click", openNewInstanceModal);
document.getElementById("lib-new-instance-btn").addEventListener("click", openNewInstanceModal);
document.getElementById("ni-cancel").addEventListener("click", () => showView("library"));

document.getElementById("ni-create").addEventListener("click", async () => {
  const name = document.getElementById("ni-name").value || "Nouvelle instance";
  const mc_version = document.getElementById("ni-version").value;
  const loader = document.getElementById("ni-loader").value;
  await invoke("create_instance", { name, mcVersion: mc_version, loader, loaderVersion: null });
  showView("library");
});

// ---------- Découvrir (recherche Modrinth) ----------
document.querySelectorAll(".tab-btn").forEach((btn) => {
  btn.addEventListener("click", () => {
    document.querySelectorAll(".tab-btn").forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    currentSearchType = btn.dataset.type;
    doSearch();
  });
});
document.getElementById("search-btn").addEventListener("click", doSearch);
document.getElementById("search-query").addEventListener("keydown", (e) => {
  if (e.key === "Enter") doSearch();
});

async function doSearch() {
  const query = document.getElementById("search-query").value;
  const results = await invoke("search_mods", {
    query,
    projectType: currentSearchType,
    gameVersion: null,
    loader: null,
  });
  const grid = document.getElementById("search-results");
  grid.innerHTML = "";
  for (const hit of results.hits) {
    const card = document.createElement("div");
    card.className = "card";
    card.innerHTML = `
      ${hit.icon_url ? `<img src="${hit.icon_url}" />` : `<div class="icon-fallback">📦</div>`}
      <h3>${hit.title}</h3>
      <p>${hit.description}</p>
      <p>${hit.downloads.toLocaleString()} téléchargements</p>
    `;
    grid.appendChild(card);
  }
}

// ---------- Serveurs locaux ----------
async function refreshServers() {
  const servers = await invoke("list_servers");
  const list = document.getElementById("servers-list");
  list.innerHTML = servers.length
    ? ""
    : `<p class="hint">Aucun serveur. Crée-en un pour jouer avec des amis sur ton réseau.</p>`;

  for (const srv of servers) {
    const running = await invoke("is_server_running", { serverId: srv.id });
    const row = document.createElement("div");
    row.className = "server-row";
    row.innerHTML = `
      <div class="info">
        <h3><span class="status-dot ${running ? "on" : ""}"></span>${srv.name}</h3>
        <p>${srv.mc_version} · ${srv.software} · port ${srv.port}</p>
      </div>
      <div class="actions">
        ${running
          ? `<button class="stop">■ Arrêter</button>`
          : `<button class="start">▶ Démarrer</button>`}
        <button class="delete">🗑</button>
      </div>
    `;
    const startBtn = row.querySelector(".start");
    const stopBtn = row.querySelector(".stop");
    const deleteBtn = row.querySelector(".delete");
    if (startBtn) startBtn.addEventListener("click", async () => {
      if (!srv.eula_accepted) {
        await invoke("install_server", { serverId: srv.id });
      }
      await invoke("start_server", { serverId: srv.id });
      refreshServers();
    });
    if (stopBtn) stopBtn.addEventListener("click", async () => {
      await invoke("stop_server", { serverId: srv.id });
      setTimeout(refreshServers, 500);
    });
    deleteBtn.addEventListener("click", async () => {
      await invoke("delete_server", { serverId: srv.id });
      refreshServers();
    });
    list.appendChild(row);
  }
}

document.getElementById("new-server-btn").addEventListener("click", async () => {
  showView("new-server");
  const manifest = await invoke("list_mc_versions");
  const select = document.getElementById("ns-version");
  select.innerHTML = manifest.versions
    .filter((v) => v.version_type === "release")
    .map((v) => `<option value="${v.id}">${v.id}</option>`)
    .join("");
});
document.getElementById("ns-cancel").addEventListener("click", () => showView("servers"));

document.getElementById("ns-create").addEventListener("click", async () => {
  if (!document.getElementById("ns-eula").checked) {
    alert("Tu dois accepter la Minecraft EULA pour installer un serveur.");
    return;
  }
  const name = document.getElementById("ns-name").value || "Serveur";
  const mc_version = document.getElementById("ns-version").value;
  const software = document.getElementById("ns-software").value;
  const port = parseInt(document.getElementById("ns-port").value, 10) || 25565;

  const srv = await invoke("create_server", { name, mcVersion: mc_version, software, port });
  await invoke("install_server", { serverId: srv.id });
  showView("servers");
  refreshServers();
});

// ---------- Init ----------
restoreAccount();
refreshHome();
