import type { ReactElement } from "react";
import type { RouteObject } from "react-router-dom";

import { Benchmarks } from "./Benchmarks";
import { Config } from "./Config";
import { HotKeys } from "./HotKeys";
import { Kernel } from "./Kernel";
import { Overview } from "./Overview";
import { Runs } from "./Runs";
import { Server } from "./Server";
import { Settings } from "./Settings";

type PrimaryNavItem = {
  path: string;
  label: string;
  tag: string;
  element: ReactElement;
  index?: boolean;
};

export const primaryNavItems: PrimaryNavItem[] = [
  {
    path: "/",
    label: "Overview",
    tag: "00",
    element: <Overview />,
    index: true
  },
  {
    path: "/benchmarks",
    label: "Benchmarks",
    tag: "01",
    element: <Benchmarks />
  },
  {
    path: "/runs",
    label: "Runs",
    tag: "02",
    element: <Runs />
  },
  {
    path: "/server",
    label: "Server",
    tag: "03",
    element: <Server />
  },
  {
    path: "/hot-keys",
    label: "Hot Keys",
    tag: "04",
    element: <HotKeys />
  },
  {
    path: "/kernel",
    label: "Kernel",
    tag: "05",
    element: <Kernel />
  },
  {
    path: "/config",
    label: "Config",
    tag: "06",
    element: <Config />
  },
  {
    path: "/settings",
    label: "Settings",
    tag: "07",
    element: <Settings />
  }
];

export const primaryRouteChildren: RouteObject[] = primaryNavItems.map((item) => {
  if (item.index) {
    return {
      index: true,
      element: item.element
    };
  }

  return {
    path: item.path.slice(1),
    element: item.element
  };
});
