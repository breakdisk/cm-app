"use client";

import { useState, useEffect, type ReactNode } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import { motion, AnimatePresence } from "framer-motion";
import {
  Map,
  Users,
  Truck,
  Building2,
  Boxes,
  BarChart3,
  Bot,
  Bell,
  MapPin,
  Package,
  Settings,
  ShieldCheck,
  Store,
  Receipt,
  Workflow,
  ChevronLeft,
  ChevronRight,
  LogOut,
  Zap,
  Menu,
  X,
} from "lucide-react";
import { cn } from "@/lib/design-system/cn";

// ─── Types ────────────────────────────────────────────────────────────────────

interface NavItem {
  label: string;
  href: string;
  icon: React.ElementType;
  badge?: string | number;
}

interface DashboardLayoutProps {
  children: ReactNode;
}

// ─── Nav config ───────────────────────────────────────────────────────────────

const NAV_ITEMS: NavItem[] = [
  { label: "Dispatch Console", href: "/dispatch",    icon: Map },
  { label: "Shipments",        href: "/shipments",   icon: Package },
  { label: "Drivers",          href: "/drivers",     icon: Users },
  { label: "Compliance",       href: "/compliance",  icon: ShieldCheck },
  { label: "Fleet",            href: "/fleet",       icon: Truck },
  { label: "Hubs",             href: "/hubs",        icon: Building2 },
  { label: "Carriers",         href: "/carriers",    icon: Boxes },
  { label: "Marketplace",      href: "/marketplace", icon: Store },
  { label: "Finance",          href: "/finance",     icon: Receipt },
  { label: "Analytics",        href: "/analytics",   icon: BarChart3 },
  { label: "AI Agents",        href: "/ai-agents",   icon: Bot },
  { label: "Automation",       href: "/automation",  icon: Workflow },
  { label: "Alerts",           href: "/alerts",      icon: Bell, badge: 3 },
  { label: "Map View",         href: "/map",         icon: MapPin },
  { label: "Settings",         href: "/settings",    icon: Settings },
];

// ─── Page title map ───────────────────────────────────────────────────────────

const PAGE_TITLE_MAP: Record<string, string> = {
  "/dispatch":   "Dispatch Console",
  "/shipments":  "Shipments",
  "/drivers":    "Drivers",
  "/compliance": "Compliance",
  "/fleet":      "Fleet",
  "/hubs":       "Hubs",
  "/carriers":   "Carriers",
  "/marketplace": "Marketplace",
  "/finance":    "Finance Oversight",
  "/analytics":  "Analytics",
  "/ai-agents":  "AI Agents",
  "/automation": "Automation",
  "/alerts":     "Alerts",
  "/map":        "Map View",
  "/settings":   "Settings",
};

function getPageTitle(pathname: string): string {
  if (PAGE_TITLE_MAP[pathname]) return PAGE_TITLE_MAP[pathname];
  const match = Object.keys(PAGE_TITLE_MAP)
    .filter((k) => pathname.startsWith(k + "/"))
    .sort((a, b) => b.length - a.length)[0];
  return match ? PAGE_TITLE_MAP[match] : "Operations";
}

function formatUtcTime(date: Date): string {
  return date.toUTCString().replace(/.*(\d{2}:\d{2}:\d{2}).*/, "$1") + " UTC";
}

// ─── Sidebar Nav Item ─────────────────────────────────────────────────────────

function NavLink({
  item,
  isActive,
  collapsed,
}: {
  item: NavItem;
  isActive: boolean;
  collapsed: boolean;
}) {
  const Icon = item.icon;

  return (
    <Link
      href={item.href}
      className={cn(
        "group relative flex items-center gap-3 rounded-lg px-3 py-2.5 transition-all duration-200",
        "text-sm font-medium",
        isActive
          ? "text-purple-plasma bg-purple-surface"
          : "text-white/50 hover:text-white/80 hover:bg-glass-200"
      )}
      style={
        isActive
          ? {
              boxShadow:
                "inset 3px 0 0 #A855F7, 0 0 12px rgba(168,85,247,0.08)",
            }
          : undefined
      }
    >
      <Icon
        className={cn(
          "h-4 w-4 flex-shrink-0 transition-colors duration-200",
          isActive
            ? "text-purple-plasma"
            : "text-white/40 group-hover:text-white/70"
        )}
      />

      <AnimatePresence initial={false}>
        {!collapsed && (
          <motion.span
            initial={{ opacity: 0, width: 0 }}
            animate={{ opacity: 1, width: "auto" }}
            exit={{ opacity: 0, width: 0 }}
            transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
            className="flex flex-1 items-center justify-between overflow-hidden whitespace-nowrap"
          >
            <span>{item.label}</span>
            {item.badge !== undefined && (
              <span
                className="ml-auto flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-2xs font-bold text-white"
                style={{
                  background: "#FF3B5C",
                  boxShadow: "0 0 6px rgba(255,59,92,0.5)",
                }}
              >
                {item.badge}
              </span>
            )}
          </motion.span>
        )}
      </AnimatePresence>

      {/* Badge in collapsed mode */}
      {collapsed && item.badge !== undefined && (
        <span
          className="absolute right-1.5 top-1.5 flex h-3.5 w-3.5 items-center justify-center rounded-full text-2xs font-bold text-white"
          style={{ background: "#FF3B5C", boxShadow: "0 0 5px rgba(255,59,92,0.5)" }}
        >
          {item.badge}
        </span>
      )}

      {/* Tooltip for collapsed state */}
      {collapsed && (
        <div
          className={cn(
            "pointer-events-none absolute left-full ml-3 z-50",
            "rounded-md border border-glass-border bg-canvas-100 px-2.5 py-1.5",
            "text-xs font-medium text-white/80 whitespace-nowrap",
            "opacity-0 group-hover:opacity-100 transition-opacity duration-150",
            "shadow-glass"
          )}
        >
          {item.label}
        </div>
      )}
    </Link>
  );
}

// ─── Layout ───────────────────────────────────────────────────────────────────

export default function DashboardLayout({ children }: DashboardLayoutProps) {
  const [collapsed, setCollapsed] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [utcTime, setUtcTime] = useState(() => formatUtcTime(new Date()));
  const pathname = usePathname();
  const pageTitle = getPageTitle(pathname);

  // Tick UTC clock
  useEffect(() => {
    const id = setInterval(() => setUtcTime(formatUtcTime(new Date())), 1000);
    return () => clearInterval(id);
  }, []);

  // Close mobile menu on route change
  useEffect(() => {
    setMobileOpen(false);
  }, [pathname]);

  return (
    <div className="flex min-h-screen bg-canvas font-sans antialiased">
      {/* ── Mobile backdrop ──────────────────────────────────────────────── */}
      <AnimatePresence>
        {mobileOpen && (
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm md:hidden"
            onClick={() => setMobileOpen(false)}
          />
        )}
      </AnimatePresence>

      {/* ── Sidebar ─────────────────────────────────────────────────────── */}
      <aside
        className={cn(
          "flex flex-shrink-0 flex-col overflow-hidden",
          "transition-all duration-[250ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
          collapsed ? "w-16" : "w-60",
          // Mobile: fixed overlay, toggled by mobileOpen
          "fixed inset-y-0 left-0 z-50",
          mobileOpen ? "translate-x-0" : "-translate-x-full",
          // Desktop: always visible, in-flow
          "md:relative md:inset-auto md:z-auto md:translate-x-0",
        )}
        style={{
          background: "rgba(5, 8, 16, 0.97)",
          backdropFilter: "blur(24px)",
          WebkitBackdropFilter: "blur(24px)",
          borderRight: "1px solid rgba(255, 255, 255, 0.06)",
        }}
      >
        {/* ── OPS CONSOLE label + Logo ──────────────────────────────────── */}
        <div
          className={cn(
            "flex h-16 flex-shrink-0 flex-col justify-center border-b border-glass-border",
            collapsed ? "items-center px-0" : "px-5"
          )}
        >
          <AnimatePresence initial={false}>
            {!collapsed && (
              <motion.span
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.15 }}
                className="mb-0.5 font-mono text-2xs font-semibold tracking-[0.2em] text-amber-signal"
                style={{ textShadow: "0 0 8px rgba(255,171,0,0.4)" }}
              >
                OPS CONSOLE
              </motion.span>
            )}
          </AnimatePresence>

          <div className="flex items-center gap-2.5 min-w-0">
            {/* Icon mark */}
            <div
              className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg"
              style={{
                background: "linear-gradient(135deg, #A855F7 0%, #00E5FF 100%)",
                boxShadow: "0 0 14px rgba(168,85,247,0.40)",
              }}
            >
              <Zap className="h-4 w-4 text-white" strokeWidth={2.5} />
            </div>

            <AnimatePresence initial={false}>
              {!collapsed && (
                <motion.div
                  initial={{ opacity: 0, width: 0 }}
                  animate={{ opacity: 1, width: "auto" }}
                  exit={{ opacity: 0, width: 0 }}
                  transition={{ duration: 0.2 }}
                  className="overflow-hidden"
                >
                  <span className="whitespace-nowrap font-heading text-sm font-bold tracking-tight text-white">
                    LogisticOS
                  </span>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>

        {/* ── Navigation ────────────────────────────────────────────────── */}
        <nav className="flex flex-1 flex-col gap-1 overflow-y-auto overflow-x-hidden p-3">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.href}
              item={item}
              isActive={
                pathname === item.href ||
                pathname.startsWith(item.href + "/")
              }
              collapsed={collapsed}
            />
          ))}
        </nav>

        {/* ── User section ──────────────────────────────────────────────── */}
        <div
          className={cn(
            "flex-shrink-0 border-t border-glass-border p-3",
            collapsed ? "flex flex-col items-center gap-2" : "space-y-2"
          )}
        >
          <div
            className={cn(
              "flex items-center gap-3 rounded-lg px-2 py-2",
              "bg-glass-100 transition-colors hover:bg-glass-200"
            )}
          >
            <div
              className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-xs font-bold text-white"
              style={{ background: "linear-gradient(135deg, #A855F7, #00E5FF)" }}
            >
              OA
            </div>
            <AnimatePresence initial={false}>
              {!collapsed && (
                <motion.div
                  initial={{ opacity: 0, width: 0 }}
                  animate={{ opacity: 1, width: "auto" }}
                  exit={{ opacity: 0, width: 0 }}
                  transition={{ duration: 0.2 }}
                  className="min-w-0 flex-1 overflow-hidden"
                >
                  <p className="truncate whitespace-nowrap text-xs font-medium text-white/80">
                    Ops Admin
                  </p>
                  <p className="truncate whitespace-nowrap text-2xs text-white/40 font-mono">
                    Admin · Super
                  </p>
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          <button
            className={cn(
              "group flex w-full items-center gap-3 rounded-lg px-3 py-2",
              "text-xs text-white/40 transition-all hover:bg-red-surface hover:text-red-signal",
              collapsed && "justify-center"
            )}
          >
            <LogOut className="h-3.5 w-3.5 flex-shrink-0" />
            <AnimatePresence initial={false}>
              {!collapsed && (
                <motion.span
                  initial={{ opacity: 0, width: 0 }}
                  animate={{ opacity: 1, width: "auto" }}
                  exit={{ opacity: 0, width: 0 }}
                  transition={{ duration: 0.2 }}
                  className="overflow-hidden whitespace-nowrap"
                >
                  Sign out
                </motion.span>
              )}
            </AnimatePresence>
          </button>
        </div>

        {/* ── Collapse toggle (desktop only) ────────────────────────────── */}
        <button
          onClick={() => setCollapsed((c) => !c)}
          className={cn(
            "absolute -right-3 top-[4.5rem] z-20 hidden md:flex",
            "h-6 w-6 items-center justify-center rounded-full",
            "border border-glass-border-bright bg-canvas-100 text-white/60",
            "transition-all hover:border-purple-plasma/50 hover:text-purple-plasma",
            "hover:shadow-[0_0_8px_rgba(168,85,247,0.4)]"
          )}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? (
            <ChevronRight className="h-3 w-3" />
          ) : (
            <ChevronLeft className="h-3 w-3" />
          )}
        </button>
      </aside>

      {/* ── Main content area ───────────────────────────────────────────── */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* ── Top header ────────────────────────────────────────────────── */}
        <header
          className="flex h-16 flex-shrink-0 items-center justify-between border-b border-glass-border px-4 md:px-6"
          style={{
            background: "rgba(5, 8, 16, 0.7)",
            backdropFilter: "blur(12px)",
            WebkitBackdropFilter: "blur(12px)",
          }}
        >
          {/* Hamburger (mobile) + Page title */}
          <div className="flex items-center gap-3">
            <button
              className="flex h-9 w-9 items-center justify-center rounded-lg border border-glass-border bg-glass-100 text-white/60 transition-all hover:bg-glass-200 hover:text-white/80 md:hidden"
              onClick={() => setMobileOpen((o) => !o)}
              aria-label="Toggle navigation"
            >
              {mobileOpen ? <X className="h-4 w-4" /> : <Menu className="h-4 w-4" />}
            </button>
            <motion.h1
              key={pageTitle}
              initial={{ opacity: 0, y: -6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
              className="font-heading text-lg font-semibold text-white"
            >
              {pageTitle}
            </motion.h1>
          </div>

          {/* Right controls — live status + UTC time */}
          <div className="flex items-center gap-4">
            {/* System Operational indicator */}
            <div className="flex items-center gap-2">
              <span className="relative flex h-2 w-2">
                <span className="absolute inline-flex h-full w-full animate-beacon rounded-full bg-green-signal opacity-75" />
                <span className="relative inline-flex h-2 w-2 rounded-full bg-green-signal" />
              </span>
              <span
                className="text-xs font-medium text-green-signal hidden sm:block"
                style={{ textShadow: "0 0 8px rgba(0,255,136,0.4)" }}
              >
                System Operational
              </span>
            </div>

            {/* Divider */}
            <div className="h-4 w-px bg-glass-border hidden sm:block" />

            {/* UTC time */}
            <span className="font-mono text-xs text-white/40 hidden sm:block tabular-nums">
              {utcTime}
            </span>

            {/* Notifications */}
            <button
              className={cn(
                "relative flex h-9 w-9 items-center justify-center rounded-lg",
                "border border-glass-border bg-glass-100 text-white/50",
                "transition-all hover:border-purple-plasma/30 hover:bg-glass-200 hover:text-white/80"
              )}
              aria-label="Alerts"
            >
              <Bell className="h-4 w-4" />
              <span
                className="absolute -right-1 -top-1 flex h-4 w-4 items-center justify-center rounded-full text-2xs font-bold text-white"
                style={{
                  background: "#FF3B5C",
                  boxShadow: "0 0 8px rgba(255,59,92,0.6)",
                }}
              >
                3
              </span>
            </button>
          </div>
        </header>

        {/* ── Page content ──────────────────────────────────────────────── */}
        <main className="flex-1 overflow-auto bg-canvas p-4 md:p-6">
          <motion.div
            key={pathname}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
          >
            {children}
          </motion.div>
        </main>
      </div>
    </div>
  );
}
