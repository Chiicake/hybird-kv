import { NavLink } from "react-router-dom";

import { primaryNavItems } from "../../routes/route-config";

export function Sidebar() {
  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <p className="sidebar__eyebrow">Perf Lab / System Console</p>
        <h1>HybridKV Workbench</h1>
        <p className="sidebar__copy">
          Desktop control shell for benchmark sessions, local node status, and
          future operator tooling.
        </p>
      </div>

      <nav className="sidebar__nav" aria-label="Primary">
        {primaryNavItems.map((item) => (
          <NavLink
            key={item.path}
            to={item.path}
            end={item.path === "/"}
            aria-label={item.label}
            className={({ isActive }) =>
              isActive ? "sidebar__link sidebar__link--active" : "sidebar__link"
            }
          >
            <span className="sidebar__tag" aria-hidden="true">{item.tag}</span>
            <span>{item.label}</span>
          </NavLink>
        ))}
      </nav>

      <section className="sidebar__panel" aria-label="Shell identity">
        <p className="sidebar__panel-label">Current mode</p>
        <strong>Local instrumentation</strong>
        <p>
          UI only for now. Runtime commands, persistence, and live telemetry land
          in later tasks.
        </p>
      </section>
    </aside>
  );
}
