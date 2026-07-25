import React from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { Activity, Database, FileArchive, FolderOpen, Network, Play, RefreshCw, RotateCw, Settings, Square, Terminal, Wifi } from "lucide-react";
import "./styles.css";

type ProcessState = {
  state: string;
  pid?: number | null;
  api_port?: number | null;
  p2p_port?: number | null;
  data_dir: string;
  log_dir: string;
};

type DashboardSnapshot = {
  process_state: string;
  status: any | null;
  mining: any | null;
  peers: any[];
  api_error?: string | null;
  core_cpu?: number | null;
  core_memory_bytes?: number | null;
  data_dir_size_bytes: number;
  log_dir_size_bytes: number;
  mock_mode: boolean;
};

const emptyConfig = {
  node_name: "Default Node",
  mode: "LocalTesting",
  api_port: 0,
  p2p_port: 19090,
  seed_peers: [] as string[],
  advertised_host: null as string | null,
  advertised_port: null as number | null,
  mining_enabled: false,
  miner_reward_address: "0000000000000000000000000000000000000000000000000000000000000000",
  data_dir: "",
  log_dir: ""
};

function shortHash(value?: string | null) {
  if (!value) return "Unavailable";
  if (value.length <= 18) return value;
  return `${value.slice(0, 10)}...${value.slice(-8)}`;
}

function bytes(value?: number | null) {
  if (!value) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function Card({ title, icon, children }: { title: string; icon: React.ReactNode; children: React.ReactNode }) {
  return <section className="panel"><div className="panel-title">{icon}<h2>{title}</h2></div>{children}</section>;
}

function Metric({ label, value }: { label: string; value: React.ReactNode }) {
  return <div className="metric"><span>{label}</span><strong>{value}</strong></div>;
}

function App() {
  const [mockMode, setMockMode] = React.useState(true);
  const [snapshot, setSnapshot] = React.useState<DashboardSnapshot | null>(null);
  const [process, setProcess] = React.useState<ProcessState | null>(null);
  const [message, setMessage] = React.useState("Ready");
  const [wizardOpen, setWizardOpen] = React.useState(false);
  const [config, setConfig] = React.useState(emptyConfig);

  const refresh = React.useCallback(async () => {
    try {
      const data = mockMode
        ? await invoke<DashboardSnapshot>("get_mock_dashboard_snapshot")
        : await invoke<DashboardSnapshot>("get_dashboard_snapshot");
      setSnapshot(data);
      if (!mockMode) setProcess(await invoke<ProcessState>("get_core_process_state"));
      setMessage(mockMode ? "Showing development mock data" : "Dashboard refreshed");
    } catch (err) {
      setMessage(String(err));
    }
  }, [mockMode]);

  React.useEffect(() => { refresh(); const id = window.setInterval(refresh, 5000); return () => window.clearInterval(id); }, [refresh]);

  async function action(name: string, fn: () => Promise<unknown>) {
    try { setMessage(`${name}...`); await fn(); await refresh(); setMessage(`${name} complete`); }
    catch (err) { setMessage(String(err)); }
  }

  const status = snapshot?.status;
  const mining = snapshot?.mining ?? status?.mining;
  const recovery = status?.recovery;

  return <main className="app-shell">
    <aside className="sidebar">
      <div className="brand"><div className="mark">V</div><div><h1>Vision</h1><p>Desktop 0.1.0 alpha</p></div></div>
      <button className="nav active"><Activity size={18}/>Dashboard</button>
      <button className="nav"><Wifi size={18}/>Networking</button>
      <button className="nav"><Terminal size={18}/>Logs</button>
      <button className="nav"><Settings size={18}/>Settings</button>
      <label className="toggle"><input type="checkbox" checked={mockMode} onChange={e => setMockMode(e.target.checked)} /> Mock mode</label>
    </aside>

    <section className="content">
      <header className="topbar">
        <div><h1>Node Manager</h1><p>{snapshot?.mock_mode ? "Development mock mode" : "Real Core mode"}</p></div>
        <div className="actions">
          <button onClick={() => action("Start", () => invoke("start_core", { request: null }))} disabled={mockMode}><Play size={18}/>Start</button>
          <button onClick={() => action("Stop", () => invoke("stop_core"))} disabled={mockMode}><Square size={18}/>Stop</button>
          <button onClick={() => action("Restart", () => invoke("restart_core", { request: null }))} disabled={mockMode}><RotateCw size={18}/>Restart</button>
          <button onClick={refresh}><RefreshCw size={18}/>Refresh</button>
        </div>
      </header>

      <div className="status-line">{message}</div>

      <div className="grid">
        <Card title="Core Process" icon={<Activity size={20}/>}> 
          <Metric label="State" value={snapshot?.process_state ?? "Unknown"}/>
          <Metric label="PID" value={process?.pid ?? "Not running"}/>
          <Metric label="API port" value={process?.api_port ?? "Private loopback"}/>
          <Metric label="P2P port" value={process?.p2p_port ?? "Not listening"}/>
        </Card>
        <Card title="Chain" icon={<Database size={20}/>}> 
          <Metric label="Height" value={status?.canonical_tip_height ?? "Unavailable"}/>
          <Metric label="Tip" value={<span title={status?.canonical_tip_hash}>{shortHash(status?.canonical_tip_hash)}</span>}/>
          <Metric label="Work" value={recovery?.local_work ?? "Unavailable"}/>
          <Metric label="State root" value={<span title={status?.cached_state_root}>{shortHash(status?.cached_state_root)}</span>}/>
        </Card>
        <Card title="Network" icon={<Network size={20}/>}> 
          <Metric label="Peers" value={status?.peer_count ?? 0}/>
          <Metric label="Durable" value={status?.durable_peer_count ?? 0}/>
          <Metric label="Inbound" value={status?.active_inbound_sessions ?? 0}/>
          <Metric label="Outbound" value={status?.active_outbound_sessions ?? 0}/>
          <Metric label="Transient" value={status?.transient_peer_count ?? 0}/>
        </Card>
        <Card title="Mining And Recovery" icon={<Play size={20}/>}> 
          <Metric label="Mining available" value={String(mining?.enabled ?? mining?.available ?? false)}/>
          <Metric label="Mining active" value={String(mining?.active ?? false)}/>
          <Metric label="Paused reason" value={mining?.paused_reason ?? "None"}/>
          <Metric label="Recovery" value={recovery?.state ?? "Unknown"}/>
        </Card>
        <Card title="Mempool And Resources" icon={<Activity size={20}/>}> 
          <Metric label="Mempool" value={status?.mempool_size ?? 0}/>
          <Metric label="CPU" value={snapshot?.core_cpu == null ? "Unavailable" : `${snapshot.core_cpu.toFixed(1)}%`}/>
          <Metric label="Memory" value={bytes(snapshot?.core_memory_bytes)}/>
          <Metric label="Data" value={bytes(snapshot?.data_dir_size_bytes)}/>
          <Metric label="Logs" value={bytes(snapshot?.log_dir_size_bytes)}/>
        </Card>
        <Card title="Support" icon={<FileArchive size={20}/>}> 
          <div className="button-stack">
            <button onClick={() => action("Generate support package", () => invoke("generate_support_package"))} disabled={mockMode}><FileArchive size={18}/>Generate Support Package</button>
            <button onClick={() => action("Open logs", () => invoke("open_logs_directory"))} disabled={mockMode}><FolderOpen size={18}/>View Logs</button>
            <button onClick={() => action("Open data", () => invoke("open_data_directory"))} disabled={mockMode}><FolderOpen size={18}/>Open Data Directory</button>
          </div>
        </Card>
      </div>

      <section className="wizard">
        <div className="wizard-header"><h2>Create Node</h2><button onClick={() => setWizardOpen(!wizardOpen)}>{wizardOpen ? "Hide" : "Open"}</button></div>
        {wizardOpen && <div className="wizard-grid">
          <label>Node name<input value={config.node_name} onChange={e => setConfig({...config, node_name: e.target.value})}/></label>
          <label>Mode<select value={config.mode} onChange={e => setConfig({...config, mode: e.target.value})}><option>LocalTesting</option><option>PrivateNetwork</option><option>InternetNetwork</option></select></label>
          <label>P2P port<input type="number" value={config.p2p_port} onChange={e => setConfig({...config, p2p_port: Number(e.target.value)})}/></label>
          <label>Seed peers<input placeholder="host:port, host:port" onChange={e => setConfig({...config, seed_peers: e.target.value.split(',').map(v => v.trim()).filter(Boolean)})}/></label>
          <label>Advertised address<input placeholder="public host or DNS" onChange={e => setConfig({...config, advertised_host: e.target.value || null})}/></label>
          <label>Mining<input type="checkbox" checked={config.mining_enabled} onChange={e => setConfig({...config, mining_enabled: e.target.checked})}/></label>
          <label className="wide">Reward address<input value={config.miner_reward_address} onChange={e => setConfig({...config, miner_reward_address: e.target.value})}/></label>
          <p className="wide note">Internet mode requires manual router forwarding in RC2. Vision Desktop will not open firewall or router ports automatically.</p>
          <button className="wide" onClick={() => action("Save node config", () => invoke("save_node_config", { request: { config } }))}>Create Node</button>
        </div>}
      </section>
    </section>
  </main>;
}

createRoot(document.getElementById("root")!).render(<App />);
