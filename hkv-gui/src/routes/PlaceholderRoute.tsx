type PlaceholderRouteProps = {
  eyebrow: string;
  title: string;
  description: string;
  dependency: string;
};

export function PlaceholderRoute(props: PlaceholderRouteProps) {
  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">{props.eyebrow}</p>
        <h1>{props.title}</h1>
        <p className="page__lede">{props.description}</p>
      </div>

      <article className="panel panel--placeholder">
        <p className="panel__label">Planned expansion</p>
        <h2>This route is intentionally a placeholder</h2>
        <p>{props.dependency}</p>
      </article>
    </section>
  );
}
