const benchmarkTracks = [
  "Runner selection surface",
  "Scenario presets and command framing",
  "Live console and metrics timeline"
];

export function Benchmarks() {
  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Benchmarks / Orchestration lane</p>
        <h1>Benchmarks</h1>
        <p className="page__lede">
          Reserved for launching and observing benchmark runs once the Tauri
          command surface exists.
        </p>
      </div>

      <article className="panel panel--stacked">
        <p className="panel__label">Shell-ready sections</p>
        <h2>Navigation is live; execution is intentionally absent</h2>
        <ul className="list-card">
          {benchmarkTracks.map((track) => (
            <li key={track}>{track}</li>
          ))}
        </ul>
      </article>
    </section>
  );
}
