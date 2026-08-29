import { invoke } from "@tauri-apps/api/core";

const $ = (id) => document.getElementById(id);

let queuedFiles = [];

async function refreshProtocol() {
  try {
    const v = await invoke("protocol_version");
    $("protocol").textContent = v;
  } catch (e) {
    $("protocol").textContent = "offline";
  }
}

async function startReceive() {
  const port = parseInt($("recv_port").value, 10) || 45843;
  const name = $("recv_name").value || "My PC";
  const dir = await prompt("Save received files to:", "~/Downloads/MorseLink");
  if (!dir) return;
  const result = await invoke("start_receive", { port, dir, deviceName: name });
  $("recv_status").textContent = result;
}

async function stopReceive() {
  await invoke("stop_receive");
  $("recv_status").textContent = "Stopped.";
}

async function refreshPeers() {
  const peers = await invoke("list_peers");
  const ul = $("peers");
  ul.innerHTML = "";
  if (peers.length === 0) {
    ul.innerHTML = "<li>No devices found yet.</li>";
    return;
  }
  for (const p of peers) {
    const li = document.createElement("li");
    li.textContent = p;
    ul.appendChild(li);
  }
}

function addFiles(files) {
  for (const f of files) {
    queuedFiles.push(f.path);
    const li = document.createElement("li");
    li.textContent = f.name;
    $("queue").appendChild(li);
  }
}

async function sendFiles() {
  const addr = $("peer_addr").value.trim();
  if (!addr || queuedFiles.length === 0) {
    alert("Provide a peer address and at least one file.");
    return;
  }
  const result = await invoke("send_files", { port: addr, files: queuedFiles });
  alert(result);
  queuedFiles = [];
  $("queue").innerHTML = "";
}

window.addEventListener("DOMContentLoaded", () => {
  refreshProtocol();

  $("start_recv").addEventListener("click", startReceive);
  $("stop_recv").addEventListener("click", stopReceive);
  $("refresh_peers").addEventListener("click", refreshPeers);
  $("send_btn").addEventListener("click", sendFiles);

  const dz = $("dropzone");
  const input = $("file_input");
  dz.addEventListener("click", () => input.click());
  dz.addEventListener("dragover", (e) => {
    e.preventDefault();
    dz.classList.add("dragging");
  });
  dz.addEventListener("dragleave", () => dz.classList.remove("dragging"));
  dz.addEventListener("drop", (e) => {
    e.preventDefault();
    dz.classList.remove("dragging");
    addFiles(e.dataTransfer.files);
  });
  input.addEventListener("change", (e) => addFiles(e.target.files));
});
