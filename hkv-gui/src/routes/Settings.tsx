import { useMemo, useState } from "react";

import {
  loadRuntimePreferences,
  saveRuntimePreferences,
  type RuntimePreferences
} from "../lib/runtime-preferences";

function previewBenchmarkBinaryPathExpansion(values: RuntimePreferences) {
  const configured = values.benchmarkBinaryPath.trim();
  const base = configured.length > 0 ? configured : "redis-benchmark";

  return [
    {
      label: "Launch token",
      value: base
    },
    {
      label: "With HOME expanded later",
      value: base.replace(/^~(?=\/|$)/, "<HOME>").replace(/\$\{HOME\}|\$HOME/g, "<HOME>")
    }
  ];
}

export function Settings() {
  const [values, setValues] = useState<RuntimePreferences>(loadRuntimePreferences);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  const previews = useMemo(() => previewBenchmarkBinaryPathExpansion(values), [values]);

  const handleChange = (field: keyof RuntimePreferences, value: string) => {
    setSavedMessage(null);
    setValues((current) => ({ ...current, [field]: value }));
  };

  const handleSave = () => {
    setValues(saveRuntimePreferences(values));
    setSavedMessage("Saved in local browser-backed app storage.");
  };

  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Settings / Runtime lane</p>
        <h1>Settings</h1>
        <p className="page__lede">
          Keep v1 intentionally small: one local benchmark executable override plus
          default benchmark target host and port.
        </p>
      </div>

      <div className="page-panel-grid">
        <article className="panel panel--accent benchmark-panel">
          <p className="panel__label">Runtime preferences</p>
          <h2>Minimal workstation overrides for v1</h2>
          <p>
            These values stay in local browser-backed storage for this desktop UI.
            Nothing else is configurable in v1.
          </p>

          <div className="benchmark-form-grid">
            <label className="benchmark-field" htmlFor="settings-benchmark-path">
              <span>Benchmark executable path override</span>
              <input
                id="settings-benchmark-path"
                value={values.benchmarkBinaryPath}
                onChange={(event) => handleChange("benchmarkBinaryPath", event.target.value)}
                placeholder="redis-benchmark"
              />
              <small className="benchmark-field__hint">
                Leave blank to launch `redis-benchmark` from PATH.
              </small>
            </label>

            <label className="benchmark-field" htmlFor="settings-target-host">
              <span>Default benchmark target host</span>
              <input
                id="settings-target-host"
                value={values.benchmarkTargetHost}
                onChange={(event) => handleChange("benchmarkTargetHost", event.target.value)}
              />
            </label>

            <label className="benchmark-field" htmlFor="settings-target-port">
              <span>Default benchmark target port</span>
              <input
                id="settings-target-port"
                inputMode="numeric"
                value={values.benchmarkTargetPort}
                onChange={(event) => handleChange("benchmarkTargetPort", event.target.value)}
              />
            </label>
          </div>

          <div className="server-actions" role="group" aria-label="Runtime preferences actions">
            <button
              type="button"
              className="server-button server-button--primary"
              onClick={handleSave}
            >
              Save runtime preferences
            </button>
          </div>

          {savedMessage ? <p>{savedMessage}</p> : null}
        </article>

        <article className="panel panel--stacked benchmark-panel">
          <p className="panel__label">Planned path expansion</p>
          <h2>Expansion preview for a future backend adapter</h2>
          <p>
            Actual path resolution depends on a later Tauri backend capability. For v1,
            this page only records the raw override and shows a conservative HOME-based
            preview we may support later.
          </p>

          <dl className="server-kv-list">
            {previews.map((preview) => (
              <div key={preview.label}>
                <dt>{preview.label}</dt>
                <dd>{preview.value}</dd>
              </div>
            ))}
          </dl>
        </article>
      </div>
    </section>
  );
}
