// SPDX-License-Identifier: AGPL-3.0-or-later

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useState } from "react";
import { api, done, errorMessage, fetchStatus, unwrap } from "./api";
import { Banner, Icon, type IconName, Loading, Logo } from "./components/ui";
import { setTimeZone } from "./lib/format";
import { startLive, stopLive, useLive } from "./lib/live";
import { Link, match, navigate, useLocation } from "./lib/router";
import { ME_KEY, UserContext, useCan, useMeQuery } from "./lib/session";
import { applyTheme, storedTheme } from "./lib/theme";
import { Account } from "./pages/Account";
import { Artists } from "./pages/Artists";
import { Categories } from "./pages/Categories";
import { ClockEditor, Clocks } from "./pages/Clocks";
import { Dashboard } from "./pages/Dashboard";
import { History } from "./pages/History";
import { Library } from "./pages/Library";
import { Login } from "./pages/Login";
import { Outputs } from "./pages/Outputs";
import { Playout } from "./pages/Playout";
import { QueuePage } from "./pages/Queue";
import { Restrictions } from "./pages/Restrictions";
import { Review } from "./pages/Review";
import { Rotation } from "./pages/Rotation";
import { Schedule } from "./pages/Schedule";
import { Settings } from "./pages/Settings";
import { Track } from "./pages/Track";
import { Users } from "./pages/Users";

applyTheme(storedTheme());

export function App() {
  const me = useMeQuery();
  const user = me.data;

  useEffect(() => {
    if (user) {
      applyTheme(user.theme);
      startLive();
    } else if (user === null) {
      stopLive();
    }
  }, [user]);

  if (me.isPending) {
    return <Loading label="Contacting Heron…" />;
  }
  if (me.isError) {
    return (
      <main className="login">
        <Banner kind="bad">The API cannot be reached: {errorMessage(me.error)}</Banner>
      </main>
    );
  }
  if (!user) {
    return <Login />;
  }
  return (
    <UserContext.Provider value={user}>
      <Shell />
    </UserContext.Provider>
  );
}

interface Page {
  path: string;
  label: string;
  icon: IconName;
  section?: string;
  admin?: boolean;
}

const PAGES: Page[] = [
  { path: "/", label: "On air", icon: "air" },
  { path: "/queue", label: "Queue", icon: "queue" },
  { path: "/history", label: "History", icon: "history" },
  { path: "/library", label: "Tracks", icon: "library", section: "Library" },
  { path: "/artists", label: "Artists", icon: "mic" },
  { path: "/review", label: "Review & organize", icon: "review" },
  { path: "/clocks", label: "Clocks", icon: "clock", section: "Programming" },
  { path: "/schedule", label: "Schedule", icon: "calendar" },
  { path: "/categories", label: "Categories", icon: "tag" },
  { path: "/restrictions", label: "Restrictions", icon: "ban" },
  { path: "/rotation", label: "Selection log", icon: "log" },
  { path: "/playout", label: "Crossfade & loudness", icon: "fade", section: "Station" },
  { path: "/outputs", label: "Outputs", icon: "output" },
  { path: "/settings", label: "Settings", icon: "settings" },
  { path: "/users", label: "Users", icon: "users", admin: true },
];

function route(pathname: string): ReactNode {
  const track = match("/library/:id", pathname);
  if (track) {
    return <Track id={Number(track.id)} />;
  }
  const clock = match("/clocks/:id", pathname);
  if (clock) {
    return <ClockEditor id={clock.id === "new" ? null : Number(clock.id)} />;
  }
  switch (pathname) {
    case "/":
      return <Dashboard />;
    case "/queue":
      return <QueuePage />;
    case "/history":
      return <History />;
    case "/library":
      return <Library />;
    case "/categories":
      return <Categories />;
    case "/artists":
      return <Artists />;
    case "/clocks":
      return <Clocks />;
    case "/schedule":
      return <Schedule />;
    case "/restrictions":
      return <Restrictions />;
    case "/rotation":
      return <Rotation />;
    case "/review":
      return <Review />;
    case "/playout":
      return <Playout />;
    case "/outputs":
      return <Outputs />;
    case "/settings":
      return <Settings />;
    case "/users":
      return <Users />;
    case "/account":
      return <Account />;
    default:
      return (
        <Banner kind="warn">
          There is no page here. <Link to="/">Back on air</Link>
        </Banner>
      );
  }
}

function isCurrent(page: string, pathname: string): boolean {
  return page === "/" ? pathname === "/" : pathname === page || pathname.startsWith(`${page}/`);
}

function Shell() {
  const { pathname } = useLocation();
  const [navOpen, setNavOpen] = useState(false);
  const isAdmin = useCan("admin");
  const client = useQueryClient();
  const user = client.getQueryData<{ username: string }>(ME_KEY);
  const review = useQuery({
    queryKey: ["review-count"],
    queryFn: () => unwrap(api.GET("/api/v1/library/review", { params: { query: { limit: 1 } } })),
    refetchInterval: 60_000,
  });
  const reviewCount = review.data?.total ?? 0;
  const station = useQuery({
    queryKey: ["settings", "station"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/station")),
  });
  // Before the pages render, so they write times in station time.
  if (station.data) {
    setTimeZone(station.data.timezone);
  }

  useEffect(() => {
    setNavOpen(false);
  }, [pathname]);

  const signOut = async () => {
    await done(api.POST("/api/v1/auth/logout")).catch(() => {});
    stopLive();
    client.setQueryData(ME_KEY, null);
    client.removeQueries({ predicate: (query) => query.queryKey[0] !== ME_KEY[0] });
    navigate("/");
  };

  let section: string | undefined;
  return (
    <div className={navOpen ? "shell nav-open" : "shell"}>
      {navOpen && (
        <div className="scrim" onClick={() => setNavOpen(false)} aria-hidden="true" />
      )}
      <aside className="sidebar" aria-label="Navigation">
        <Link to="/" className="brand">
          <Logo /> Heron
        </Link>
        <nav className="nav">
          {PAGES.filter((page) => !page.admin || isAdmin).map((page) => {
            const heading = page.section !== undefined && page.section !== section;
            if (page.section) {
              section = page.section;
            }
            return (
              <div key={page.path} style={{ display: "contents" }}>
                {heading && <div className="nav-section">{page.section}</div>}
                <Link
                  to={page.path}
                  aria-current={isCurrent(page.path, pathname) ? "page" : undefined}
                >
                  <Icon name={page.icon} /> {page.label}
                  {page.path === "/review" && reviewCount > 0 && (
                    <span className="count" title="Tracks waiting for review">
                      {reviewCount}
                    </span>
                  )}
                </Link>
              </div>
            );
          })}
        </nav>
        <div className="sidebar-footer">
          <nav className="nav">
            <Link to="/account" aria-current={pathname === "/account" ? "page" : undefined}>
              <Icon name="user" /> <span className="ellipsis">{user?.username ?? "Account"}</span>
            </Link>
            <a
              href="/"
              onClick={(event) => {
                event.preventDefault();
                void signOut();
              }}
            >
              <Icon name="logout" /> Sign out
            </a>
          </nav>
        </div>
      </aside>
      <div className="main">
        <div className="topbar">
          <button
            type="button"
            className="btn ghost icon"
            aria-label="Menu"
            aria-expanded={navOpen}
            onClick={() => setNavOpen(true)}
          >
            <Icon name="menu" />
          </button>
          <Link to="/" className="brand">
            <Logo size={20} /> Heron
          </Link>
          <span className="spacer" />
          <OnAirBadge />
        </div>
        <main className="content">
          <Warnings />
          {route(pathname)}
        </main>
      </div>
    </div>
  );
}

function OnAirBadge() {
  const player = useLive((live) => live.player);
  const onAir = player?.decks.some((deck) => deck.on_air) && !player.paused;
  return onAir ? (
    <span className="on-air-pill">
      <span className="dot" style={{ background: "#fff" }} /> ON AIR
    </span>
  ) : null;
}

/** Problems worth knowing on every page. */
function Warnings() {
  const connected = useLive((live) => live.connected);
  const player = useLive((live) => live.player);
  const status = useQuery({ queryKey: ["status"], queryFn: () => fetchStatus(), refetchInterval: 15_000 });
  const engine = useQuery({
    queryKey: ["engine"],
    queryFn: () => unwrap(api.GET("/api/v1/engine")),
    refetchInterval: 15_000,
  });
  const [disconnectedSince, setDisconnectedSince] = useState<number | null>(null);
  useEffect(() => {
    setDisconnectedSince(connected ? null : Date.now());
  }, [connected]);
  // Brief reconnections are not worth a banner.
  const [, tick] = useState(0);
  useEffect(() => {
    if (connected) {
      return;
    }
    const timer = setTimeout(() => tick((n) => n + 1), 3_000);
    return () => clearTimeout(timer);
  }, [connected]);

  const warnings: ReactNode[] = [];
  if (status.isError) {
    warnings.push(<>The API cannot be reached: {errorMessage(status.error)}</>);
  } else if (!connected && disconnectedSince !== null && Date.now() - disconnectedSince > 2_500) {
    warnings.push(<>Live updates are disconnected. Reconnecting…</>);
  }
  if (status.data?.database === "unavailable") {
    warnings.push(<>The database is unavailable: the station plays on, but nothing can be changed.</>);
  }
  if (player === null || status.data?.engine === "unavailable") {
    warnings.push(<>The engine cannot be reached. Its outputs keep streaming what it has queued.</>);
  }
  if (engine.data?.config.state === "failed") {
    warnings.push(
      <>
        The engine refused the station's configuration: {engine.data.config.error}.{" "}
        <Link to="/outputs">Check the outputs</Link>
      </>,
    );
  }
  const clock = status.data?.clock.warning;
  if (clock) {
    warnings.push(
      <>
        <strong>Clock:</strong> {clock}. Scheduled programming may start at the wrong time.
      </>,
    );
  }
  return (
    <>
      {warnings.map((warning, index) => (
        <Banner key={index} kind={index === 0 && status.isError ? "bad" : "warn"}>
          {warning}
        </Banner>
      ))}
    </>
  );
}
