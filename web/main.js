import QRCode from "qrcode";

const $ = (id) => document.getElementById(id);

let pc = null;
let dc = null;
let files = [];

const CHUNK = 256 * 1024;

function show(id, yes) {
  $(id).style.display = yes ? "block" : "none";
}

async function createOffer() {
  pc = new RTCPeerConnection({ iceServers: [] });
  dc = pc.createDataChannel("morselink", { ordered: true });
  dc.binaryType = "arraybuffer";

  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  const sdp = JSON.stringify(offer);
  $("offer_text").value = sdp;
  QRCode.toCanvas($("qr_canvas"), sdp, { width: 220, margin: 2 });
  show("offer_panel", true);
  show("answer_panel", false);
  show("send_panel", true);

  pc.onicecandidate = (e) => {};
  dc.onopen = () => ready();
}

async function acceptOffer() {
  pc = new RTCPeerConnection({ iceServers: [] });
  pc.ondatachannel = (e) => {
    dc = e.channel;
    dc.binaryType = "arraybuffer";
    dc.onopen = () => ready();
    dc.onmessage = handleMessage;
  };

  const offer = JSON.parse($("remote_offer").value);
  await pc.setRemoteDescription(offer);
  const answer = await pc.createAnswer();
  await pc.setLocalDescription(answer);
  $("answer_text").value = JSON.stringify(answer);
  show("answer_block", true);
  pc.onicecandidate = () => {};
}

async function acceptAnswer() {
  const answer = JSON.parse(prompt("Paste the receiver's answer:"));
  await pc.setRemoteDescription(answer);
}

function ready() {
  dc.onmessage = (e) => handleMessage(e);
  show("progress_panel", true);
  $("status_text").textContent = "Connected. Choose files to send.";
  if (dc && dc.readyState === "open" && files.length) {
    sendFiles();
  }
}

function handleMessage(evt) {
  const data = typeof evt.data === "string" ? evt.data : null;
  if (data === "__BEGIN__") {
    $("status_text").textContent = "Receiving file…";
    return;
  }
  if (data && data.startsWith("__META__:")) {
    $("status_text").textContent = `Receiving ${data.slice(9)}`;
    return;
  }
  if (data === "__END__") {
    $("status_text").textContent = "Transfer complete.";
    return;
  }
  if (evt.data instanceof ArrayBuffer) {
    const arr = new Uint8Array(evt.data);
    $("progress").value = 1; // full-file; real impl would accumulate chunks
  }
}

async function sendFiles() {
  if (!dc || dc.readyState !== "open") {
    alert("Not connected yet.");
    return;
  }
  dc.send("__META__:" + files.map((f) => f.name).join(", "));
  for (const file of files) {
    dc.send("__BEGIN__");
    const buffer = await file.arrayBuffer();
    let offset = 0;
    while (offset < buffer.byteLength) {
      const part = buffer.slice(offset, Math.min(offset + CHUNK, buffer.byteLength));
      dc.send(part);
      offset += CHUNK;
    }
    dc.send("__END__");
  }
  $("status_text").textContent = "Done.";
}

function addFiles(list) {
  files = [...list];
  $("queue").innerHTML = "";
  for (const f of list) {
    const li = document.createElement("li");
    li.textContent = `${f.name} (${(f.size / 1024).toFixed(1)} KB)`;
    $("queue").appendChild(li);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  $("create_file").addEventListener("click", createOffer);
  $("create_receive").addEventListener("click", () => {
    show("offer_panel", false);
    show("answer_panel", true);
    show("send_panel", false);
  });
  $("accept_offer").addEventListener("click", acceptOffer);
  $("accept_answer_btn")?.addEventListener("click", acceptAnswer);

  const dz = $("dropzone");
  const input = $("file_input");
  dz.addEventListener("click", () => input.click());
  dz.addEventListener("dragover", (e) => { e.preventDefault(); dz.classList.add("dragging"); });
  dz.addEventListener("dragleave", () => dz.classList.remove("dragging"));
  dz.addEventListener("drop", (e) => { e.preventDefault(); dz.classList.remove("dragging"); addFiles(e.dataTransfer.files); });
  input.addEventListener("change", (e) => addFiles(e.target.files));
});
