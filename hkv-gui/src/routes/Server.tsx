const serverAreas = [
  "Local start and stop controls",
  "INFO snapshot summary",
  "Process and address state"
];

export function Server() {
  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Server / Local runtime lane</p>
        <h1>Server</h1>
        <p className="page__lede">
          Dedicated frame for future local server lifecycle controls and light
          status polling.
        </p>
      </div>

      <article className="panel panel--stacked">
        <p className="panel__label">Reserved operator surfaces</p>
        <h2>No fake toggles before process management exists</h2>
        <ul className="list-card">
          {serverAreas.map((area) => (
            <li key={area}>{area}</li>
          ))}
        </ul>
      </article>
    </section>
  );
}
