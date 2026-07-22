<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  // Scaffold-level smoke test: confirm the Rust backend is reachable.
  // Real sensor wiring arrives on feature/sensor-core + feature/lhm-bridge.
  let backend = $state<string>("connecting…");

  onMount(async () => {
    try {
      backend = await invoke<string>("app_info");
    } catch (e) {
      backend = `backend unavailable: ${e}`;
    }
  });
</script>

<main class="shell">
  <header class="titlebar">
    <span class="app-name">SensorView</span>
    <span class="subtitle">Hardware Monitor</span>
  </header>

  <div class="body">
    <aside class="tree">
      <div class="tree-header">Sensors</div>
      <div class="placeholder">Hardware tree — coming on feature/ui-sensors-table</div>
    </aside>
    <section class="content">
      <div class="placeholder">
        <p>Scaffold running.</p>
        <p class="dim">Backend: {backend}</p>
      </div>
    </section>
  </div>

  <footer class="statusbar">
    <span>{backend}</span>
  </footer>
</main>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .titlebar {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 6px 10px;
    background: var(--bg-header);
    border-bottom: 1px solid var(--grid);
  }
  .app-name {
    font-weight: 600;
    color: var(--accent);
  }
  .subtitle {
    color: var(--text-dim);
    font-size: 11px;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .tree {
    width: 260px;
    background: var(--bg-panel);
    border-right: 1px solid var(--grid);
    overflow: auto;
  }
  .tree-header {
    padding: 4px 8px;
    background: var(--bg-header);
    color: var(--text-dim);
    text-transform: uppercase;
    font-size: 10px;
    letter-spacing: 0.5px;
  }
  .content {
    flex: 1;
    overflow: auto;
  }
  .placeholder {
    padding: 16px;
    color: var(--text-dim);
  }
  .dim {
    color: var(--text-dim);
  }
  .statusbar {
    padding: 3px 10px;
    background: var(--bg-header);
    border-top: 1px solid var(--grid);
    color: var(--text-dim);
    font-size: 11px;
  }
</style>
