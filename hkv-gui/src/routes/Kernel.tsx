import { PlaceholderRoute } from "./PlaceholderRoute";

export function Kernel() {
  return (
    <PlaceholderRoute
      eyebrow="Kernel / Runtime lane"
      title="Kernel"
      description="Lower-level engine inspection stays out of v1 because the desktop app does not yet expose native runtime internals."
      dependency="Depends on future backend models for engine state, instrumentation hooks, and a safe Tauri command surface for kernel data."
    />
  );
}
