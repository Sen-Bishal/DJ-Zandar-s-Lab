const cycleCountEl = document.getElementById("cycle-count");
const entropyNowEl = document.getElementById("entropy-now");
const timeConceptEl = document.getElementById("time-concept");
const sampleCountEl = document.getElementById("sample-count");
const entropyPolyline = document.getElementById("entropy-polyline");

const CHART_WIDTH = 960;
const CHART_HEIGHT = 320;
const MAX_POINTS = 600;
const POLL_MS = 75;

const invokeFromGlobal =
  window.__TAURI__ &&
  window.__TAURI__.core &&
  typeof window.__TAURI__.core.invoke === "function"
    ? window.__TAURI__.core.invoke
    : null;

const invokeFromInternals =
  window.__TAURI_INTERNALS__ &&
  typeof window.__TAURI_INTERNALS__.invoke === "function"
    ? (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args)
    : null;

const invoke = invokeFromGlobal || invokeFromInternals;

if (!invoke) {
  timeConceptEl.textContent = "tauri bridge unavailable";
}

let inFlight = false;

function renderChart(samples) {
  const points = [];
  const count = Math.max(samples.length, 2);

  for (let i = 0; i < samples.length; i += 1) {
    const x = count <= 1 ? 0 : (i / (count - 1)) * CHART_WIDTH;
    const y = CHART_HEIGHT - Math.max(0, Math.min(1, samples[i])) * CHART_HEIGHT;
    points.push(`${x.toFixed(2)},${y.toFixed(2)}`);
  }

  entropyPolyline.setAttribute("points", points.join(" "));
}

async function pollState() {
  if (!invoke || inFlight) {
    return;
  }

  inFlight = true;
  try {
    const [state, samplesRaw] = await Promise.all([
      invoke("read_global_state"),
      invoke("read_entropy_series"),
    ]);

    const samples = Array.isArray(samplesRaw)
      ? samplesRaw.slice(-MAX_POINTS)
      : [];

    cycleCountEl.textContent = String(state.cycle_count);
    entropyNowEl.textContent = Number(state.destruction_entropy).toFixed(6);
    timeConceptEl.textContent = state.time_concept_active ? "active" : "bypassed";
    sampleCountEl.textContent = String(samples.length);
    renderChart(samples);
  } catch (err) {
    timeConceptEl.textContent = "observer link error";
    console.error("observer poll failed", err);
  } finally {
    inFlight = false;
  }
}

setInterval(pollState, POLL_MS);
pollState();
