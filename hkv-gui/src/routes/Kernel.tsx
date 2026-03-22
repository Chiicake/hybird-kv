import { PlaceholderRoute } from "./PlaceholderRoute";

export function Kernel() {
  return (
    <PlaceholderRoute
      eyebrow="Kernel / Runtime lane"
      title="Kernel"
      description="Lower-level engine and runtime inspection surfaces arrive in a later phase."
      dependency="Depends on backend models, instrumentation hooks, and native runtime data that arrive in later tasks."
    />
  );
}
