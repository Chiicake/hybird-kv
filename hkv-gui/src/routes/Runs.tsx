const runViews = [
  "History list and latest summaries",
  "Single-run detail frame",
  "Future comparison workflow"
];

export function Runs() {
  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Runs / Archive lane</p>
        <h1>Runs</h1>
        <p className="page__lede">
          This route is the future archive for benchmark sessions and normalized
          result summaries.
        </p>
      </div>

      <article className="panel panel--stacked">
        <p className="panel__label">Planned surfaces</p>
        <h2>Results stay empty until persistence arrives</h2>
        <ul className="list-card">
          {runViews.map((view) => (
            <li key={view}>{view}</li>
          ))}
        </ul>
      </article>
    </section>
  );
}
