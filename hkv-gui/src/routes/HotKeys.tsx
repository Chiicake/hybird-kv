import { PlaceholderRoute } from "./PlaceholderRoute";

export function HotKeys() {
  return (
    <PlaceholderRoute
      eyebrow="Hot Keys / Keyboard lane"
      title="Hot Keys"
      description="Keyboard-first control paths stay out of v1 because the shell cannot yet register or persist desktop shortcuts."
      dependency="Depends on a future Tauri global-shortcut bridge, command routing for focused actions, and saved keymap preferences."
    />
  );
}
