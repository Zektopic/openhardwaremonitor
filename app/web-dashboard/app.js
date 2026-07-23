// SensorView dashboard client.
//
// Opens /ws/telemetry and renders each frame. Rows are created once and then
// mutated in place — rebuilding the DOM at 1 Hz makes text unselectable and
// scroll position jump, which is exactly wrong for a monitoring view.
(function () {
  "use strict";

  // The token (when the server is LAN-exposed) rides in the query string,
  // because a browser cannot set headers when opening a page or a socket.
  var token = new URLSearchParams(location.search).get("token");

  var els = {
    root: document.getElementById("root"),
    status: document.getElementById("status"),
    host: document.getElementById("host"),
    stats: document.getElementById("stats"),
    engine: document.getElementById("engine"),
    seq: document.getElementById("seq"),
  };

  // identifier -> { valueCell, canvas, samples }
  var rows = new Map();
  var groupsKey = "";
  var socket = null;
  var retryMs = 500;

  function setStatus(state, text) {
    els.status.className = "status " + state;
    els.status.textContent = text || state;
  }

  function wsUrl() {
    var proto = location.protocol === "https:" ? "wss:" : "ws:";
    var url = proto + "//" + location.host + "/ws/telemetry";
    return token ? url + "?token=" + encodeURIComponent(token) : url;
  }

  function connect() {
    setStatus("connecting");
    socket = new WebSocket(wsUrl());

    socket.onopen = function () {
      retryMs = 500; // successful connect resets the backoff
      setStatus("online");
    };

    socket.onmessage = function (ev) {
      var frame;
      try {
        frame = JSON.parse(ev.data);
      } catch (e) {
        return; // ignore a malformed frame rather than tearing down the view
      }
      render(frame);
    };

    socket.onclose = function () {
      setStatus("offline");
      // Exponential backoff, capped, so a closed laptop doesn't hammer the app.
      setTimeout(connect, retryMs);
      retryMs = Math.min(retryMs * 2, 10000);
    };

    socket.onerror = function () {
      if (socket) socket.close();
    };
  }

  // ---- rendering ---------------------------------------------------------

  function flatten(tree, out, parent) {
    for (var i = 0; i < tree.length; i++) {
      var hw = tree[i];
      var title = parent ? parent + " — " + hw.name : hw.name;
      if (hw.sensors && hw.sensors.length) {
        out.push({ title: title, sensors: hw.sensors });
      }
      if (hw.sub_hardware && hw.sub_hardware.length) {
        flatten(hw.sub_hardware, out, hw.name);
      }
    }
    return out;
  }

  function format(value, type) {
    if (value === null || value === undefined) return "—";
    var abs = Math.abs(value);
    // Voltages need three decimals to be useful; RPM and MHz never do.
    if (type === "Voltage") return value.toFixed(3);
    if (type === "Fan" || type === "Clock" || type === "Frequency") return value.toFixed(0);
    if (abs >= 1000) return value.toFixed(0);
    if (abs >= 100) return value.toFixed(1);
    return value.toFixed(1);
  }

  var UNITS = {
    Voltage: "V", Current: "A", Clock: "MHz", Temperature: "°C", Load: "%",
    Frequency: "Hz", Fan: "RPM", Flow: "L/h", Control: "%", Level: "%",
    Factor: "", Power: "W", Data: "GB", SmallData: "MB", Throughput: "MB/s",
    TimeSpan: "s", Energy: "mWh", Noise: "dBA", Conductivity: "µS/cm", Humidity: "%",
  };

  function build(groups) {
    els.root.textContent = "";
    rows.clear();

    groups.forEach(function (g) {
      var section = document.createElement("section");
      section.className = "group";

      var h = document.createElement("h2");
      h.textContent = g.title;
      section.appendChild(h);

      var table = document.createElement("table");
      g.sensors.forEach(function (s) {
        var tr = document.createElement("tr");

        var name = document.createElement("td");
        name.className = "name";
        name.textContent = s.name;
        name.title = s.identifier;

        var value = document.createElement("td");
        value.className = "value t-" + s.type;

        var spark = document.createElement("td");
        spark.className = "spark";
        var canvas = document.createElement("canvas");
        canvas.className = "sparkline";
        spark.appendChild(canvas);

        tr.appendChild(name);
        tr.appendChild(value);
        tr.appendChild(spark);
        table.appendChild(tr);

        rows.set(s.identifier, { value: value, canvas: canvas, samples: [] });
      });

      section.appendChild(table);
      els.root.appendChild(section);
    });
  }

  function update(groups) {
    groups.forEach(function (g) {
      g.sensors.forEach(function (s) {
        var row = rows.get(s.identifier);
        if (!row) return;

        var unit = UNITS[s.type] || "";
        row.value.textContent = format(s.value, s.type);
        if (unit) {
          var span = document.createElement("span");
          span.className = "unit";
          span.textContent = unit;
          row.value.appendChild(span);
        }
        row.value.title =
          "min " + format(s.min, s.type) +
          " · max " + format(s.max, s.type) +
          " · avg " + format(s.avg, s.type);

        if (typeof s.value === "number" && isFinite(s.value)) {
          row.samples.push(s.value);
          if (row.samples.length > 120) row.samples.shift();
          sparkline(row.canvas, row.samples, s.type);
        }
      });
    });
  }

  var TYPE_COLOR = {
    Temperature: "#f08060", Voltage: "#f0d030", Clock: "#60c8f0",
    Fan: "#80d080", Power: "#e8c050",
  };

  function sparkline(canvas, samples, type) {
    // Match the backing store to the CSS box so the line isn't blurry on HiDPI.
    var dpr = window.devicePixelRatio || 1;
    var w = canvas.clientWidth, h = canvas.clientHeight;
    if (!w || !h) return;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr;
      canvas.height = h * dpr;
    }

    var ctx = canvas.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    if (samples.length < 2) return;

    var min = Infinity, max = -Infinity;
    for (var i = 0; i < samples.length; i++) {
      if (samples[i] < min) min = samples[i];
      if (samples[i] > max) max = samples[i];
    }
    // A flat series would divide by zero; draw it down the middle instead.
    var span = max - min || 1;

    ctx.beginPath();
    for (var j = 0; j < samples.length; j++) {
      var x = (j / (samples.length - 1)) * w;
      var y = h - 1 - ((samples[j] - min) / span) * (h - 2);
      if (j === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.strokeStyle = TYPE_COLOR[type] || "#4fa3ff";
    ctx.lineWidth = 1;
    ctx.stroke();
  }

  function render(frame) {
    if (frame.error) {
      setStatus("offline", "error");
      return;
    }
    var groups = flatten(frame.tree || [], [], null);

    // Rebuild only when the sensor set itself changed (device hotplug, backend
    // switch) — otherwise mutate in place.
    var key = groups.map(function (g) {
      return g.title + ":" + g.sensors.map(function (s) { return s.identifier; }).join(",");
    }).join("|");
    if (key !== groupsKey) {
      groupsKey = key;
      build(groups);
    }
    update(groups);

    var sensors = groups.reduce(function (n, g) { return n + g.sensors.length; }, 0);
    els.host.textContent = location.host;
    els.stats.textContent = groups.length + " devices · " + sensors + " sensors";
    els.seq.textContent = "frame #" + frame.seq;
    els.engine.textContent =
      (frame.source || "") +
      (frame.diagnostics && frame.diagnostics.engine_version
        ? " · " + frame.diagnostics.engine_version
        : "");
  }

  // Resync promptly when a backgrounded tab comes back, instead of showing
  // whatever was on screen when it was hidden.
  document.addEventListener("visibilitychange", function () {
    if (!document.hidden && socket && socket.readyState === WebSocket.OPEN) {
      socket.send("snapshot");
    }
  });

  connect();
})();
