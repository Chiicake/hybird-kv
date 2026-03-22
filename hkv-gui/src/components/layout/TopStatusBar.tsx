export function TopStatusBar() {
  return (
    <header className="top-status-bar">
      <div>
        <p className="top-status-bar__label">Workbench state</p>
        <p className="top-status-bar__value">Shell online / backend pending</p>
      </div>

      <div className="top-status-bar__chips" aria-label="Status indicators">
        <span className="status-chip status-chip--ok">UI scaffold ready</span>
        <span className="status-chip">Server controls later</span>
        <span className="status-chip">No live process attached</span>
      </div>
    </header>
  );
}
