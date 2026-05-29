import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const BANDS = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
const LABELS = ["32", "64", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];

const PRESETS = {
  flat:    { label: "Flat",       values: [0,0,0,0,0,0,0,0,0,0] },
  esport:  { label: "Esport / CS",values: [-2,0,0,2,5,5,4,3,2,1] },
  music:   { label: "Music",      values: [3,2,1,0,-1,0,1,2,2,1] },
  podcast: { label: "Podcast",    values: [-2,-2,0,3,4,3,2,0,-1,-2] },
};

const ACCENT = "#e8441a";
const ACCENT2 = "#f07820";

export default function App() {
  const canvasRef = useRef(null);
  const [vals, setVals] = useState([...PRESETS.flat.values]);
  const [dragging, setDragging] = useState(-1);
  const [eqOn, setEqOn] = useState(true);
  const [preamp, setPreamp] = useState(0);
  const [activePreset, setActivePreset] = useState("flat");
  const [customPresets, setCustomPresets] = useState({});
  const [showModal, setShowModal] = useState(false);
  const [modalName, setModalName] = useState("");
  const [activeDevice, setActiveDevice] = useState("Detecting...");
  const [apoFound, setApoFound] = useState(false);

  const valsRef = useRef(vals);
  const draggingRef = useRef(dragging);
  valsRef.current = vals;
  draggingRef.current = dragging;

  useEffect(() => {
  setActiveDevice("APO not detected");
  setApoFound(false);
}, []);

  // Write config whenever vals, preamp, or eqOn changes
  useEffect(() => {
    if (!eqOn) {
      invoke("write_eq_config", { config: "Preamp: 0 dB\n" }).catch(console.error);
      return;
    }
    const lines = [`Preamp: ${preamp >= 0 ? "+" : ""}${preamp.toFixed(1)} dB`];
    vals.forEach((gain, i) => {
      lines.push(
        `Filter ${i + 1}: ON PK Fc ${BANDS[i]} Hz Gain ${gain >= 0 ? "+" : ""}${gain.toFixed(1)} dB Q 1.41`
      );
    });
    invoke("write_eq_config", { config: lines.join("\n") + "\n" }).catch(console.error);
  }, [vals, preamp, eqOn]);

  // Canvas drawing
  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    const dpr = window.devicePixelRatio || 1;
    const W = canvas.width / dpr;
    const H = canvas.height / dpr;

    const bX = (i) => 34 + (i / (BANDS.length - 1)) * (W - 68);
    const bY = (v) => 16 + ((12 - v) / 24) * (H - 34);

    ctx.clearRect(0, 0, W, H);
    ctx.globalAlpha = eqOn ? 1 : 0.3;

    // Grid lines
    ctx.strokeStyle = "rgba(255,255,255,0.05)";
    ctx.lineWidth = 0.5;
    [-12, -6, 0, 6, 12].forEach((db) => {
      const y = bY(db);
      ctx.beginPath(); ctx.moveTo(34, y); ctx.lineTo(W - 34, y); ctx.stroke();
      ctx.fillStyle = "rgba(255,255,255,0.2)";
      ctx.font = "10px system-ui";
      ctx.textAlign = "right";
      ctx.fillText((db > 0 ? "+" : "") + db, 28, y + 3.5);
    });

    ctx.strokeStyle = "rgba(255,255,255,0.04)";
    BANDS.forEach((_, i) => {
      const x = bX(i);
      ctx.beginPath(); ctx.moveTo(x, 12); ctx.lineTo(x, H - 20); ctx.stroke();
      ctx.fillStyle = "rgba(255,255,255,0.22)";
      ctx.font = "10px system-ui";
      ctx.textAlign = "center";
      ctx.fillText(LABELS[i], x, H - 5);
    });

    const currentVals = valsRef.current;
    const pts = currentVals.map((_, i) => [bX(i), bY(currentVals[i])]);

    // Fill under curve
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (let i = 0; i < pts.length - 1; i++) {
      const mx = (pts[i][0] + pts[i + 1][0]) / 2;
      ctx.bezierCurveTo(mx, pts[i][1], mx, pts[i + 1][1], pts[i + 1][0], pts[i + 1][1]);
    }
    ctx.lineTo(W - 34, H - 20); ctx.lineTo(34, H - 20); ctx.closePath();
    const grad = ctx.createLinearGradient(0, 0, 0, H);
    grad.addColorStop(0, "rgba(232,68,26,0.22)");
    grad.addColorStop(1, "rgba(232,68,26,0.01)");
    ctx.fillStyle = grad; ctx.fill();

    // Curve line
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (let i = 0; i < pts.length - 1; i++) {
      const mx = (pts[i][0] + pts[i + 1][0]) / 2;
      ctx.bezierCurveTo(mx, pts[i][1], mx, pts[i + 1][1], pts[i + 1][0], pts[i + 1][1]);
    }
    ctx.strokeStyle = ACCENT; ctx.lineWidth = 2; ctx.stroke();

    // Band dots
    currentVals.forEach((v, i) => {
      const x = bX(i), y = bY(v);
      const hot = draggingRef.current === i;
      ctx.beginPath();
      ctx.arc(x, y, hot ? 8 : 5.5, 0, Math.PI * 2);
      ctx.fillStyle = v !== 0 || hot ? ACCENT : "rgba(255,255,255,0.1)";
      ctx.fill();
      ctx.strokeStyle = hot ? ACCENT2 : "rgba(255,255,255,0.2)";
      ctx.lineWidth = 1.5; ctx.stroke();

      if (v !== 0) {
        ctx.fillStyle = "rgba(255,255,255,0.85)";
        ctx.font = "500 10px system-ui";
        ctx.textAlign = "center";
        ctx.fillText((v > 0 ? "+" : "") + v.toFixed(1), x, v > 0 ? y - 13 : y + 19);
      }
    });

    ctx.globalAlpha = 1;
  }, [eqOn]);

  useEffect(() => { draw(); }, [vals, eqOn, draw]);

  // Canvas resize
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.parentElement.clientWidth;
      canvas.width = w * dpr;
      canvas.height = 220 * dpr;
      canvas.style.width = w + "px";
      canvas.style.height = "220px";
      const ctx = canvas.getContext("2d");
      ctx.scale(dpr, dpr);
      draw();
    };
    resize();
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, [draw]);

  // Drag helpers
  const getCanvasPos = (e, canvas) => {
    const rect = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const W = canvas.width / dpr;
    const bX = (i) => 34 + (i / (BANDS.length - 1)) * (W - 68);
    const cx = (e.touches ? e.touches[0].clientX : e.clientX) - rect.left;
    const cy = (e.touches ? e.touches[0].clientY : e.clientY) - rect.top;
    return { x: cx, y: cy, bX };
  };

  const nearBand = (x, bX) => {
    let best = -1, bd = 999;
    BANDS.forEach((_, i) => { const d = Math.abs(bX(i) - x); if (d < bd) { bd = d; best = i; } });
    return bd < 30 ? best : -1;
  };

  const yToVal = (y, canvas) => {
    const dpr = window.devicePixelRatio || 1;
    const H = canvas.height / dpr;
    const raw = 12 - ((y - 16) / (H - 34)) * 24;
    return Math.round(Math.max(-12, Math.min(12, raw)) * 2) / 2;
  };

  const handleMouseDown = (e) => {
    const canvas = canvasRef.current;
    const { x, y, bX } = getCanvasPos(e, canvas);
    const band = nearBand(x, bX);
    if (band >= 0) {
      setDragging(band);
      draggingRef.current = band;
      setVals((prev) => { const n = [...prev]; n[band] = yToVal(y, canvas); return n; });
    }
  };

  const handleMouseMove = (e) => {
    if (draggingRef.current < 0) return;
    const canvas = canvasRef.current;
    const { y } = getCanvasPos(e, canvas);
    const band = draggingRef.current;
    setVals((prev) => { const n = [...prev]; n[band] = yToVal(y, canvas); return n; });
  };

  const handleMouseUp = () => { setDragging(-1); draggingRef.current = -1; draw(); };

  const applyPreset = (key, values) => {
    setVals([...values]);
    setActivePreset(key);
  };

  const savePreset = () => {
    const name = modalName.trim();
    if (!name) return;
    const key = "custom_" + name.toLowerCase().replace(/\s+/g, "_");
    setCustomPresets((prev) => ({ ...prev, [key]: { label: name, values: [...vals] } }));
    setActivePreset(key);
    setShowModal(false);
    setModalName("");
  };

  const reset = () => {
    setVals([...PRESETS.flat.values]);
    setPreamp(0);
    setActivePreset("flat");
  };

  const allPresets = { ...PRESETS, ...customPresets };

  return (
    <div className="app">
      {/* Titlebar */}
      <div className="titlebar">
        <div className="tl">
          <div className="logo-dot" />
          <span className="app-name">Quick Equalizer</span>
        </div>
        <div className="win-controls">
          <div className="wc wc-close" />
          <div className="wc wc-min" />
          <div className="wc wc-max" />
        </div>
      </div>

      {/* Device status bar */}
      <div className="device-bar">
        <div className="device-status">
          <div className={`device-pip ${apoFound ? "pip-on" : "pip-off"}`} />
          <span className="device-text">Active device</span>
          <span className="device-name">{activeDevice}</span>
          {apoFound && <span className="apo-badge">APO active</span>}
        </div>
        <span className="device-hint">To change device, run APO setup</span>
      </div>

      {/* Presets */}
      <div className="preset-row">
        {Object.entries(allPresets).map(([key, preset]) => (
          <button
            key={key}
            className={`preset-btn ${activePreset === key ? "active" : ""}`}
            onClick={() => applyPreset(key, preset.values)}
          >
            {preset.label}
          </button>
        ))}
        <button className="save-btn" onClick={() => setShowModal(true)}>
          ＋ Save preset
        </button>
      </div>

      {/* EQ Canvas */}
      <div className="eq-area">
        <canvas
          ref={canvasRef}
          id="eq"
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
          onTouchStart={(e) => { e.preventDefault(); handleMouseDown(e); }}
          onTouchMove={(e) => { e.preventDefault(); handleMouseMove(e); }}
          onTouchEnd={handleMouseUp}
          style={{ cursor: "crosshair", display: "block", width: "100%" }}
        />
      </div>

      {/* Bottom row */}
      <div className="bottom-row">
        <div className="toggle-wrap" onClick={() => setEqOn((v) => !v)}>
          <div className={`tog ${eqOn ? "" : "tog-off"}`} />
          <span className="toggle-label">{eqOn ? "EQ on" : "EQ off"}</span>
        </div>
        <div className="preamp-wrap">
          <span className="preamp-label">Preamp</span>
          <input
            type="range"
            className="preamp-slider"
            min={-12} max={12} step={0.5}
            value={preamp}
            onChange={(e) => setPreamp(parseFloat(e.target.value))}
          />
          <span className="preamp-val">{(preamp >= 0 ? "+" : "") + preamp.toFixed(1)} dB</span>
        </div>
        <button className="reset-btn" onClick={reset}>Reset</button>
      </div>

      {/* Save preset modal */}
      {showModal && (
        <div className="modal-overlay">
          <div className="modal">
            <div className="modal-title">Save preset</div>
            <input
              className="modal-input"
              placeholder="Preset name..."
              maxLength={30}
              value={modalName}
              onChange={(e) => setModalName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && savePreset()}
              autoFocus
            />
            <div className="modal-btns">
              <button className="modal-cancel" onClick={() => setShowModal(false)}>Cancel</button>
              <button className="modal-confirm" onClick={savePreset}>Save</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
