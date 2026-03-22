const highlights = [
  {
    label: "Focus",
    value: "Local operator deck"
  },
  {
    label: "Design lane",
    value: "Perf lab shell with expansion slots"
  },
  {
    label: "Current scope",
    value: "Navigation, page framing, visual identity"
  }
];

export function Overview() {
  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Overview / Control deck</p>
        <h1>Overview</h1>
        <p className="page__lede">
          Control room for local HybridKV work. This shell establishes the visual
          frame for benchmark orchestration, run history, and server operations.
        </p>
      </div>

      <div className="metric-grid">
        {highlights.map((item) => (
          <article key={item.label} className="metric-card">
            <p className="metric-card__label">{item.label}</p>
            <strong>{item.value}</strong>
          </article>
        ))}
      </div>

      <div className="page-panel-grid">
        <article className="panel">
          <p className="panel__label">Next active surfaces</p>
          <h2>Benchmarks, runs, and server pages are wired for navigation</h2>
          <p>
            They stay skeletal in this task, but the shell is ready for later Tauri
            commands, charts, and persisted run state.
          </p>
        </article>
        <article className="panel panel--accent">
          <p className="panel__label">Why this shell exists</p>
          <h2>Make desktop control feel like an instrument, not a CRUD dashboard</h2>
          <p>
            The layout leans into console framing, operator labels, and restrained
            diagnostics rather than fake data tables or pretend controls.
          </p>
        </article>
      </div>
    </section>
  );
}
