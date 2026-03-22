import { PlaceholderRoute } from "./PlaceholderRoute";

export function HotKeys() {
  return (
    <PlaceholderRoute
      eyebrow="Hot Keys / Keyboard lane"
      title="Hot Keys"
      description="Keyboard-first control paths arrive once desktop command routing is in place."
      dependency="Depends on Tauri command bindings and runtime services that do not exist in this task."
    />
  );
}
