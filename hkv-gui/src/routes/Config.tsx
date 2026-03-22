import { PlaceholderRoute } from "./PlaceholderRoute";

export function Config() {
  return (
    <PlaceholderRoute
      eyebrow="Config / Schema lane"
      title="Config"
      description="Editable configuration arrives once the schema and persistence flows are real."
      dependency="Depends on validated config models and save workflows that are out of scope for this shell task."
    />
  );
}
