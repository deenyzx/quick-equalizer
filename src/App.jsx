import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const BANDS = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
const LABELS = ["32", "64", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];

const DEFAULT_PRESETS = {
  flat:     { label: "Flat",     values: [ 0,  0,  0,  0,  0,  0,  0,  0,  0,  0] },
  cs2:      { label: "CS2",      values: [-4, -2,  5,  6,  5,  2, -3,  6,  1, -3] },
  valorant: { label: "Valorant", values: [-4, -2,  4,  5,  4,  5, -2,  6,  4, -2] },
  r6s:      { label: "R6S",      values: [-5, -2,  6,  7,  6,  3, -4,  4,  2, -4] },
  pubg:     { label: "PUBG",     values: [-3,  2,  5,  6,  4,  2, -2,  3,  3, -1] },
  league:   { label: "League",   values: [-6, -4, -2,  0,  4,  6,  3,  5,  4,  1] },
  fortnite: { label: "Fortnite", values: [-4, -1,  4,  5,  3,  3, -3,  5,  4,  0] },
  esport:   { label: "Esport",   values: [-4, -2,  3,  5,  5,  4, -2,  5,  2, -2] },
  music:    { label: "Music",    values: [ 3,  2,  1,  0, -1,  0,  1,  2,  2,  1] },
  podcast:  { label: "Podcast",  values: [-2, -2,  0,  3,  4,  3,  2,  0, -1, -2] },
};

const ACCENT = "#e8441a";
const ACCENT2 = "#f07820";

// Canvas layout constants
const PAD_LEFT = 50;
const PAD_RIGHT = 50;
const PAD_TOP = 50;
const PAD_BOTTOM = 28;

export default function App() {
  const canvasRef = useRef(null);
  const [vals, setVals] = useState([...DEFAULT_PRESETS.flat.values]);
  const [dragging, setDragging] = useState(-1);
  const [focusedBand, setFocusedBand] = useState(-1);
  const [eqOn, setEqOn] = useState(true);
  const [preamp, setPreamp] = useState(0);
  const [activePreset, setActivePreset] = useState("flat");
  const [builtinPresets, setBuiltinPresets] = useState({ ...DEFAULT_PRESETS });
  const [customPresets, setCustomPresets] = useState({});
  const [showModal, setShowModal] = useState(false);
  const [modalName, setModalName] = useState("");
  const [activeDevice, setActiveDevice] = useState("Detecting...");
  const [apoFound, setApoFound] = useState(false);
  const [showDeviceSetup, setShowDeviceSetup] = useState(false);
  const [audioDevices, setAudioDevices] = useState([]);
  const [selectedDevice, setSelectedDevice] = useState(null);
  const [deviceLoading, setDeviceLoading] = useState(false);
  const [deviceError, setDeviceError] = useState("");
  const [setupBusy, setSetupBusy] = useState(false);
  const [setupMsg, setSetupMsg] = useState("");

  const valsRef = useRef(vals);
  const draggingRef = useRef(dragging);
  const focusedBandRef = useRef(focusedBand);
  valsRef.current = vals;
  draggingRef.current = dragging;
  focusedBandRef.current = focusedBand;

  useEffect(() => {
    invoke("is_device_configured").then((configured) => {
      if (!configured) openDeviceSetup();
    });
  }, []);

  const openDeviceSetup = () => {
    setDeviceError("");
    setDeviceLoading(true);
    setShowDeviceSetup(true);
    invoke("get_audio_devices")
      .then((devices) => {
        setAudioDevices(devices);
        if (devices.length > 0) setSelectedDevice(devices[0]);
        setDeviceLoading(false);
      })
      .catch(() => setDeviceLoading(false));
  };

  const runEngineSetup = () => {
    setSetupBusy(true);
    setSetupMsg("Setting up… approve the Windows prompt.");
    invoke("run_engine_setup")
      .then(() => {
        invoke("is_apo_installed").then(setApoFound).catch(() => {});
        setSetupMsg("Audio engine set up. A restart may be required for it to take effect.");
      })
      .catch(() => {
        // Surface the per-step report so failures are visible, not silent.
        invoke("read_engine_log")
          .then((log) => setSetupMsg(log || "Setup failed."))
          .catch(() => setSetupMsg("Setup failed."));
      })
      .finally(() => setSetupBusy(false));
  };

  const confirmDevice = () => {
    if (!selectedDevice) return;
    setDeviceError("");
    invoke("configure_device", {
      deviceId: selectedDevice.id,
      deviceName: selectedDevice.name,
    })
      .then(() => {
        setActiveDevice(selectedDevice.name);
        setShowDeviceSetup(false);
        runEngineSetup();
      })
      .catch((e) => {
        console.error("Failed to configure device:", e);
        setDeviceError(typeof e === "string" ? e : "Failed to configure device");
      });
  };

  useEffect(() => {
    invoke("is_apo_installed").then(setApoFound).catch(() => setApoFound(false));

    // Show the user's configured device if there is one, otherwise the live default.
    invoke("get_configured_device")
      .then((name) => {
        if (name) setActiveDevice(name);
        else return invoke("get_default_device_name").then(setActiveDevice);
      })
      .catch(() =>
        invoke("get_default_device_name")
          .then(setActiveDevice)
          .catch(() => setActiveDevice("Unknown device"))
      );

    invoke("load_presets")
      .then((json) => {
        const saved = JSON.parse(json);
        if (Object.keys(saved).length > 0) setCustomPresets(saved);
      })
      .catch(console.error);

    // Restore which built-in presets the user has deleted (issue #4).
    invoke("load_hidden_builtins")
      .then((json) => {
        const hidden = JSON.parse(json);
        if (Array.isArray(hidden) && hidden.length > 0) {
          setBuiltinPresets((prev) => {
            const next = { ...prev };
            hidden.forEach((k) => delete next[k]);
            return next;
          });
        }
      })
      .catch(console.error);
  }, []);

  useEffect(() => {
    // Nothing to write to when EqualizerAPO isn't installed.
    if (!apoFound) return;
    if (!eqOn) {
      invoke("write_eq_config", { config: "Preamp: 0 dB\n" }).catch(console.error);
      return;
    }
    const lines = [`Preamp: ${preamp >= 0 ? "+" : ""}${preamp.toFixed(1)} dB`];
    vals.forEach((gain, i) => {
      lines.push(`Filter ${i + 1}: ON PK Fc ${BANDS[i]} Hz Gain ${gain >= 0 ? "+" : ""}${gain.toFixed(1)} dB Q 1.41`);
    });
    invoke("write_eq_config", { config: lines.join("\n") + "\n" }).catch(console.error);
  }, [vals, preamp, eqOn, apoFound]);

  const bX = useCallback((i, W) => PAD_LEFT + (i / (BANDS.length - 1)) * (W - PAD_LEFT - PAD_RIGHT), []);
  const bY = useCallback((v, H) => PAD_TOP + ((12 - v) / 24) * (H - PAD_TOP - PAD_BOTTOM), []);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    const dpr = window.devicePixelRatio || 1;
    const W = canvas.width / dpr;
    const H = canvas.height / dpr;

    ctx.clearRect(0, 0, W, H);
    ctx.globalAlpha = eqOn ? 1 : 0.3;

    // dB grid lines
    ctx.lineWidth = 0.5;
    [-12, -6, 0, 6, 12].forEach((db) => {
      const y = bY(db, H);
      ctx.strokeStyle = db === 0 ? "rgba(255,255,255,0.12)" : "rgba(255,255,255,0.0991)";
      ctx.beginPath(); ctx.moveTo(PAD_LEFT, y); ctx.lineTo(W - PAD_RIGHT, y); ctx.stroke();
      ctx.fillStyle = "rgba(255, 255, 255, 0.99)";
      ctx.font = "10px system-ui";
      ctx.textAlign = "right";
      ctx.fillText((db > 0 ? "+" : "") + db + " dB", PAD_LEFT - 10, y + 3.5);
    });

    // Vertical band lines + frequency labels
    BANDS.forEach((_, i) => {
      const x = bX(i, W);
      ctx.strokeStyle = "rgba(255,255,255,0.099)";
      ctx.lineWidth = 0.5;
      ctx.beginPath(); ctx.moveTo(x, PAD_TOP); ctx.lineTo(x, H - PAD_BOTTOM + 4); ctx.stroke();
      ctx.fillStyle = "rgba(255,255,255,0.8)";
      ctx.font = "10px system-ui";
      ctx.textAlign = "center";
      const freqLabel = BANDS[i] >= 1000 ? (BANDS[i]/1000) + "kHz" : BANDS[i] + "Hz";
      ctx.fillText(freqLabel, x, H - 6);
    });

    const currentVals = valsRef.current;
    const pts = currentVals.map((_, i) => [bX(i, W), bY(currentVals[i], H)]);

    // Filled area under curve
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (let i = 0; i < pts.length - 1; i++) {
      const mx = (pts[i][0] + pts[i + 1][0]) / 2;
      ctx.bezierCurveTo(mx, pts[i][1], mx, pts[i + 1][1], pts[i + 1][0], pts[i + 1][1]);
    }
    ctx.lineTo(W - PAD_RIGHT, H - PAD_BOTTOM);
    ctx.lineTo(PAD_LEFT, H - PAD_BOTTOM);
    ctx.closePath();
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
    const focused = focusedBandRef.current;
    currentVals.forEach((v, i) => {
      const x = bX(i, W), y = bY(v, H);
      const hot = draggingRef.current === i;
      const isFocused = focused === i;

      // Focus ring
      if (isFocused) {
        ctx.beginPath();
        ctx.arc(x, y, 11, 0, Math.PI * 2);
        ctx.strokeStyle = "rgba(232,68,26,0.4)";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }

      ctx.beginPath();
      ctx.arc(x, y, hot ? 8 : 5.5, 0, Math.PI * 2);
      ctx.fillStyle = v !== 0 || hot || isFocused ? ACCENT : "rgba(255,255,255,0.1)";
      ctx.fill();
      ctx.strokeStyle = hot ? ACCENT2 : isFocused ? "#fff" : "rgba(255,255,255,0.2)";
      ctx.lineWidth = 1.5; ctx.stroke();

      // Always show value label for focused band, otherwise only if non-zero
      if (v !== 0 || isFocused) {
        ctx.fillStyle = isFocused ? "#fff" : "rgba(255,255,255,0.85)";
        ctx.font = isFocused ? "600 10px system-ui" : "500 10px system-ui";
        ctx.textAlign = "center";
        const label = (v > 0 ? "+" : "") + v.toFixed(1) + " dB";
        ctx.fillText(label, x, v >= 0 ? y - 13 : y + 19);
      }
    });

    ctx.globalAlpha = 1;
  }, [eqOn, bX, bY]);

  useEffect(() => { draw(); }, [vals, eqOn, focusedBand, draw]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.parentElement.clientWidth;
      canvas.width = w * dpr;
      canvas.height = 330 * dpr;
      canvas.style.width = w + "px";
      canvas.style.height = "330px";
      const ctx = canvas.getContext("2d");
      ctx.scale(dpr, dpr);
      draw();
    };
    resize();
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, [draw]);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e) => {
      if (showModal || showDeviceSetup) return;
      const focused = focusedBandRef.current;

      if (e.key === "ArrowRight") {
        e.preventDefault();
        const next = focused < 0 ? 0 : Math.min(BANDS.length - 1, focused + 1);
        setFocusedBand(next);
        focusedBandRef.current = next;
        draw();
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        const prev = focused < 0 ? 0 : Math.max(0, focused - 1);
        setFocusedBand(prev);
        focusedBandRef.current = prev;
        draw();
      } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && focused >= 0) {
        e.preventDefault();
        const delta = e.key === "ArrowUp" ? 0.1 : -0.1;
        setVals((prev) => {
          const n = [...prev];
          n[focused] = Math.round(Math.max(-12, Math.min(12, n[focused] + delta)) * 10) / 10;
          return n;
        });
      } else if (e.key === "Escape") {
        setFocusedBand(-1);
        focusedBandRef.current = -1;
        draw();
      } else if (e.key === "Enter" || e.key === "Escape") {
        setFocusedBand(-1);
       focusedBandRef.current = -1;
        draw();
}
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [draw, showModal, showDeviceSetup]);

  const getCanvasPos = (e, canvas) => {
    const rect = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const W = canvas.width / dpr;
    const cx = (e.touches ? e.touches[0].clientX : e.clientX) - rect.left;
    const cy = (e.touches ? e.touches[0].clientY : e.clientY) - rect.top;
    return { x: cx, y: cy, W };
  };

  const nearBand = (x, W) => {
    let best = -1, bd = 999;
    BANDS.forEach((_, i) => { const d = Math.abs(bX(i, W) - x); if (d < bd) { bd = d; best = i; } });
    return bd < 30 ? best : -1;
  };

  const yToVal = (y, canvas) => {
    const dpr = window.devicePixelRatio || 1;
    const H = canvas.height / dpr;
    const raw = 12 - ((y - PAD_TOP) / (H - PAD_TOP - PAD_BOTTOM)) * 24;
    return Math.round(Math.max(-12, Math.min(12, raw)) * 10) / 10;
  };

  const handleMouseDown = (e) => {
    const canvas = canvasRef.current;
    const { x, y, W } = getCanvasPos(e, canvas);
    const band = nearBand(x, W);
    if (band < 0) {
    // clicked away from any band
    setFocusedBand(-1);
    focusedBandRef.current = -1;
    }
    else if (band >= 0) {
      setDragging(band);
      draggingRef.current = band;
      setFocusedBand(band);
      focusedBandRef.current = band;
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

  const applyPreset = (key, values) => { setVals([...values]); setActivePreset(key); };

  const savePreset = () => {
    const name = modalName.trim();
    if (!name) return;
    const key = "custom_" + name.toLowerCase().replace(/\s+/g, "_");
    const newPresets = { ...customPresets, [key]: { label: name, values: [...vals] } };
    setCustomPresets(newPresets);
    setActivePreset(key);
    setShowModal(false);
    setModalName("");
    invoke("save_presets", { json: JSON.stringify(newPresets) }).catch(console.error);
  };

  const deletePreset = (key) => {
    if (builtinPresets[key]) {
      const updated = { ...builtinPresets };
      delete updated[key];
      setBuiltinPresets(updated);
      // Persist the deletion so the built-in doesn't reappear on restart (issue #4).
      const hidden = Object.keys(DEFAULT_PRESETS).filter((k) => !updated[k]);
      invoke("save_hidden_builtins", { json: JSON.stringify(hidden) }).catch(console.error);
      if (activePreset === key) setActivePreset(Object.keys(updated)[0] || "");
      return;
    }
    const updated = { ...customPresets };
    delete updated[key];
    setCustomPresets(updated);
    if (activePreset === key) setActivePreset(Object.keys(builtinPresets)[0] || "");
    invoke("save_presets", { json: JSON.stringify(updated) }).catch(console.error);
  };

  const reset = () => { setVals([0,0,0,0,0,0,0,0,0,0]); setPreamp(0); setActivePreset(""); };

  const allPresets = { ...builtinPresets, ...customPresets };

  return (
    <div className="app">
      <div className="titlebar">
        <div className="tl">
          <div className="logo-dot" />
          <span className="app-name">Quick Equalizer</span>
        </div>
        <div className="win-controls">
          <div className="wc wc-min" onClick={() => invoke("minimize_window")}>─</div>
          <div className="wc wc-max" onClick={() => invoke("maximize_window")}>🗖</div>
          <div className="wc wc-close" onClick={() => invoke("close_window")}>✕</div>
        </div>
      </div>

      <div className="device-bar">
        <div className="device-status">
          <div className={`device-pip ${apoFound ? "pip-on" : "pip-off"}`} />
          <span className="device-text">Active device</span>
          <span className="device-name">{activeDevice}</span>
          {apoFound ? (
            <span className="apo-badge">Active</span>
          ) : (
            <span className="apo-badge apo-badge-off">Setup needed</span>
          )}
        </div>
        <button className="change-device-btn" onClick={openDeviceSetup}>
          {apoFound ? "Change device" : "Set up"}
        </button>
      </div>

      {setupMsg && (
        <div
          style={{
            padding: "7px 18px",
            fontSize: 11,
            color: "rgba(255,255,255,0.6)",
            whiteSpace: "pre-wrap",
            borderBottom: "0.5px solid rgba(255,255,255,0.08)",
            fontFamily: "ui-monospace, monospace",
          }}
        >
          {setupMsg}
        </div>
      )}

      <div className="preset-row">
        {Object.entries(allPresets).map(([key, preset]) => (
          <div key={key} className="preset-wrap">
            <button
              className={`preset-btn ${activePreset === key ? "active" : ""}`}
              onClick={() => applyPreset(key, preset.values)}
            >
              {preset.label}
            </button>
            <button className="preset-delete" onClick={() => deletePreset(key)}>🗑</button>
          </div>
        ))}
        <button className="save-btn" onClick={() => setShowModal(true)}>＋ Save preset</button>
      </div>

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

      <div className="bottom-row">
        <div className="toggle-wrap" onClick={() => setEqOn((v) => !v)}>
          <div className={`tog ${eqOn ? "" : "tog-off"}`} />
          <span className="toggle-label">{eqOn ? "EQ on" : "EQ off"}</span>
        </div>
        <div className="preamp-wrap">
          <span className="preamp-label">Preamp</span>
          <input
            type="range" className="preamp-slider"
            min={-12} max={12} step={0.1} value={preamp}
            onChange={(e) => setPreamp(parseFloat(e.target.value))}
          />
          <span className="preamp-val">{(preamp >= 0 ? "+" : "") + preamp.toFixed(1)} dB</span>
        </div>
        <button className="reset-btn" onClick={reset}>Reset</button>
      </div>

      {showModal && (
        <div className="modal-overlay">
          <div className="modal">
            <div className="modal-title">Save preset</div>
            <input
              className="modal-input" placeholder="Preset name..."
              maxLength={30} value={modalName}
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

      {showDeviceSetup && (
        <div className="modal-overlay">
          <div className="modal" style={{ width: 320 }}>
            <div className="modal-title">Select audio device</div>
            <p style={{ fontSize: 11, color: "rgba(255,255,255,0.4)", marginBottom: 12 }}>
              Choose which device the EQ should process. You can change this later.
            </p>
            {!apoFound && (
              <p style={{ fontSize: 11, color: "#f0a020", marginBottom: 12 }}>
                The audio engine isn't set up yet — your selection is saved and will apply once
                setup completes.
              </p>
            )}
            {deviceError && (
              <p style={{ fontSize: 11, color: "#e8441a", marginBottom: 12 }}>{deviceError}</p>
            )}
            {deviceLoading ? (
              <div style={{ color: "rgba(255,255,255,0.4)", fontSize: 12, marginBottom: 12 }}>Detecting devices...</div>
            ) : (
              <div className="device-list">
                {audioDevices.map((d) => (
                  <div
                    key={d.id}
                    className={`device-option ${selectedDevice?.id === d.id ? "selected" : ""}`}
                    onClick={() => setSelectedDevice(d)}
                  >
                    {d.name}
                  </div>
                ))}
              </div>
            )}
            <div className="modal-btns">
              {apoFound && (
                <button className="modal-cancel" onClick={() => setShowDeviceSetup(false)}>Cancel</button>
              )}
              <button className="modal-confirm" onClick={confirmDevice} disabled={!selectedDevice}>Apply</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}