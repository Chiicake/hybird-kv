import { useId } from "react";

import type { BenchmarkFormValues } from "./form-model";

type BenchmarkFormProps = {
  busy: boolean;
  errors: Partial<Record<keyof BenchmarkFormValues, string>>;
  values: BenchmarkFormValues;
  onChange: (field: keyof BenchmarkFormValues, value: string) => void;
  onSubmit: () => void;
};

export function BenchmarkForm({ busy, errors, values, onChange, onSubmit }: BenchmarkFormProps) {
  const formId = useId();
  const usesDuration = values.executionMode === "duration";

  return (
    <article className="panel panel--accent benchmark-panel">
      <p className="panel__label">Benchmark configuration</p>
      <h2>Run a focused workbench profile</h2>
      <p>
        Keep v1 modest: target one endpoint, choose a profile, and launch the
        existing backend runner with concrete request settings.
      </p>

      <div className="benchmark-form-grid">
        <label className="benchmark-field" htmlFor={`${formId}-host`}>
          <span>Target host</span>
          <input
            id={`${formId}-host`}
            value={values.host}
            onChange={(event) => onChange("host", event.target.value)}
          />
          {errors.host ? <small className="field-error">{errors.host}</small> : null}
        </label>

        <label className="benchmark-field" htmlFor={`${formId}-port`}>
          <span>Target port</span>
          <input
            id={`${formId}-port`}
            inputMode="numeric"
            value={values.port}
            onChange={(event) => onChange("port", event.target.value)}
          />
          {errors.port ? <small className="field-error">{errors.port}</small> : null}
        </label>

        <label className="benchmark-field" htmlFor={`${formId}-mode`}>
          <span>Benchmark mode</span>
          <select
            id={`${formId}-mode`}
            value={values.executionMode}
            onChange={(event) => onChange("executionMode", event.target.value)}
          >
            <option value="requests">Request count</option>
            <option value="duration">Duration</option>
          </select>
        </label>

        <label className="benchmark-field" htmlFor={`${formId}-profile`}>
          <span>Benchmark profile</span>
          <select
            id={`${formId}-profile`}
            value={values.profile}
            onChange={(event) => onChange("profile", event.target.value)}
          >
            <option value="throughput">Throughput</option>
            <option value="latency">Latency</option>
            <option value="balanced">Balanced</option>
          </select>
        </label>

        {usesDuration ? (
          <label className="benchmark-field" htmlFor={`${formId}-duration`}>
            <span>Duration</span>
            <input
              id={`${formId}-duration`}
              inputMode="numeric"
              value={values.durationSeconds}
              onChange={(event) => onChange("durationSeconds", event.target.value)}
            />
            <small className="benchmark-field__hint">
              v1 maps duration to a request budget before launching the backend runner.
            </small>
            {errors.durationSeconds ? <small className="field-error">{errors.durationSeconds}</small> : null}
          </label>
        ) : (
          <label className="benchmark-field" htmlFor={`${formId}-requests`}>
            <span>Request count</span>
            <input
              id={`${formId}-requests`}
              inputMode="numeric"
              value={values.requests}
              onChange={(event) => onChange("requests", event.target.value)}
            />
            {errors.requests ? <small className="field-error">{errors.requests}</small> : null}
          </label>
        )}

        <label className="benchmark-field" htmlFor={`${formId}-clients`}>
          <span>Concurrency</span>
          <input
            id={`${formId}-clients`}
            inputMode="numeric"
            value={values.clients}
            onChange={(event) => onChange("clients", event.target.value)}
          />
          {errors.clients ? <small className="field-error">{errors.clients}</small> : null}
        </label>

        <label className="benchmark-field" htmlFor={`${formId}-payload`}>
          <span>Payload size</span>
          <input
            id={`${formId}-payload`}
            inputMode="numeric"
            value={values.dataSize}
            onChange={(event) => onChange("dataSize", event.target.value)}
          />
          {errors.dataSize ? <small className="field-error">{errors.dataSize}</small> : null}
        </label>

        <label className="benchmark-field" htmlFor={`${formId}-pipeline`}>
          <span>Pipeline depth</span>
          <input
            id={`${formId}-pipeline`}
            inputMode="numeric"
            value={values.pipeline}
            onChange={(event) => onChange("pipeline", event.target.value)}
          />
          {errors.pipeline ? <small className="field-error">{errors.pipeline}</small> : null}
        </label>
      </div>

      <div className="server-actions" role="group" aria-label="Benchmark actions">
        <button
          type="button"
          className="server-button server-button--primary"
          onClick={onSubmit}
          disabled={busy}
        >
          {busy ? "Starting..." : "Start benchmark"}
        </button>
      </div>
    </article>
  );
}
