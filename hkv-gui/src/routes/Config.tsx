import { PlaceholderRoute } from "./PlaceholderRoute";

export function Config() {
  return (
    <PlaceholderRoute
      eyebrow="Config / Schema lane"
      title="Config"
      description="Editable config is not real in v1 because there is no validated schema editor or save/apply workflow behind this route yet."
      dependency="Depends on future backend schema models, validation errors from Tauri commands, and explicit load-save-apply flows."
    />
  );
}
